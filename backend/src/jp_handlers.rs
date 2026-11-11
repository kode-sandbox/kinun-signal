//! Handlers and batch logic for the Japanese-stock version (/api/jp/*).
//!
//! Data flow per code:
//! J-Quants (prices / listed master / financial summary incl. company
//! forecasts) + EDINET (detailed BS/PL/CF figures) -> StockFinancials
//! (JPY) -> the existing, unmodified `dcf::calculate` -> jp_* tables.
//!
//! Parameter policy for the trial (LLM re-implementation is out of scope):
//! - WACC: the existing `dcf::calculate_auto_wacc` (CAPM) is reused. Its
//! constants were tuned for US stocks (ERP, risk-free blending), and the
//! risk-free rate is a fixed JP placeholder below — both are TODO items
//! to recalibrate for JGB/TOPIX once real data flows (spec D-JP4).
//! - Growth rates: derived from the company forecast in the J-Quants
//! summary (forecast revenue vs latest revenue); bull/bear are spreads
//! around base. Terminal growth reuses `calculate_auto_terminal_growth`.

use std::sync::Arc;

use axum::{
extract::{Path, State},
http::StatusCode,
response::IntoResponse,
Json,
};
use chrono::Utc;
use serde_json::json;
use tracing::{error, warn};

use crate::{
db_jp,
dcf::{self, DcfInput},
handlers::AppState,
models::{
AddJpWatchlistRequest, JobSummaryResponse, JpStockDetailResponse, JpTheoreticalPriceRow,
JpWatchlistItemResponse, JpWatchlistRow,
},
services::{
edinet::EdinetDcfDetails,
fmp::StockFinancials,
jquants::{JpDailyBar, JpFinSummary},
},
};

type ApiResult<T> = Result<T, (StatusCode, Json<serde_json::Value>)>;

/// TODO(D-JP4): replace with the live 10y JGB yield (MoF CSV is the leading
/// candidate). Placeholder long-run JP risk-free rate for the trial.
const JP_RISK_FREE_RATE: f64 = 0.025;

fn internal_err<E: std::fmt::Display>(err: E) -> (StatusCode, Json<serde_json::Value>) {
error!("JP API error: {}", err);
(
StatusCode::INTERNAL_SERVER_ERROR,
Json(json!({ "error": err.to_string()})),
)
}

fn not_found_err(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
(StatusCode::NOT_FOUND, Json(json!({ "error": msg})))
}

fn bad_request(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
(StatusCode::BAD_REQUEST, Json(json!({ "error": msg})))
}

fn normalize_code(raw: &str) -> Option<String> {
let code = raw.trim().to_uppercase();
if code.is_empty() || code.len() > 8 ||!code.chars().all(|c| c.is_ascii_alphanumeric()) {
None
} else {
Some(code)
}
}

/// Fallback DCF details derived from the J-Quants summary when EDINET data
/// cannot be obtained (e.g. zip extraction not wired, no recent filing).
/// Proxies are deliberately conservative and logged as such.
fn fallback_details_from_summary(summary: &JpFinSummary) -> EdinetDcfDetails {
let total_assets = summary.total_assets.unwrap_or(0.0);
let net_assets = summary.net_assets.unwrap_or(0.0);
EdinetDcfDetails {
cash_and_short_term_investments: 0.0,
// Liabilities as a debt proxy (overstates interest-bearing debt).
total_debt: (total_assets - net_assets).max(0.0),
operating_cash_flow: summary.net_income.unwrap_or(0.0),
capex: 0.0,
// Net income as an FCF proxy until EDINET CF data is available.
free_cash_flow: summary.net_income.unwrap_or(0.0),
shares_outstanding: summary.shares_outstanding.unwrap_or(0.0),
revenue: summary.revenue,
interest_expense: None,
effective_tax_rate: None,
}
}

/// Bundle J-Quants summary + EDINET details into the shared StockFinancials
/// shape (all JPY), so the existing DCF code is reused unchanged.
pub async fn build_jp_financials(
state: &AppState,
code: &str,
edinet_code: Option<&str>,
) -> anyhow::Result<(StockFinancials, Option<JpFinSummary>, Option<JpDailyBar>)> {
let summary = state.jquants.fetch_latest_summary(code).await?;
let details = match state.edinet.fetch_latest_dcf_details(code, edinet_code).await {
Ok(d) => d,
Err(err) => {
warn!("EDINET details unavailable for {code} ({err}); using summary-derived fallback");
match &summary {
Some(s) => fallback_details_from_summary(s),
None => anyhow::bail!("No financial data for {code} from J-Quants or EDINET"),
}
}
};
let bar = state.jquants.fetch_latest_bar(code).await?;
let listed = state.jquants.fetch_listed_info(code).await?;

let shares = if details.shares_outstanding > 0.0 {
details.shares_outstanding
} else {
summary
.as_ref()
.and_then(|s| s.shares_outstanding)
.unwrap_or(0.0)
};
let revenue = details
.revenue
.or_else(|| summary.as_ref().and_then(|s| s.revenue))
.unwrap_or(0.0);

let fin = StockFinancials {
ticker: code.to_string(),
company_name: listed
.map(|l| l.company_name)
.filter(|n|!n.is_empty())
.unwrap_or_else(|| code.to_string()),
current_price: bar.as_ref().map(|b| b.close).unwrap_or(0.0),
cash_and_short_term_investments: details.cash_and_short_term_investments,
total_debt: details.total_debt,
free_cash_flow: details.free_cash_flow,
fcf_history: vec![details.free_cash_flow],
revenue_history: vec![revenue],
shares_outstanding: shares,
latest_earnings_date: summary.as_ref().and_then(|s| s.disclosed_date),
eps_actual: summary.as_ref().and_then(|s| s.eps),
eps_estimated: summary.as_ref().and_then(|s| s.forecast_eps),
beta: None, // TODO: compute vs TOPIX from J-Quants daily bars (monthly closes).
risk_free_rate: Some(JP_RISK_FREE_RATE),
effective_tax_rate: details.effective_tax_rate,
interest_expense: details.interest_expense,
};
Ok((fin, summary, bar))
}

struct JpDcfParams {
wacc: f64,
growth_rate_base: f64,
growth_rate_bull: f64,
growth_rate_bear: f64,
terminal_growth_rate: f64,
reasoning_ja: String,
}

fn derive_jp_params(fin: &StockFinancials, summary: Option<&JpFinSummary>) -> JpDcfParams {
let wacc_comp = dcf::calculate_auto_wacc(
fin.current_price,
fin.shares_outstanding,
fin.total_debt,
fin.beta,
fin.risk_free_rate,
fin.interest_expense,
fin.effective_tax_rate,
);
// Base growth from the company forecast (forecast revenue vs actual).
let forecast_growth = match summary {
Some(s) => match (s.forecast_revenue, s.revenue) {
(Some(fc), Some(rev)) if rev > 0.0 => Some((fc / rev - 1.0).clamp(-0.05, 0.15)),
_ => None,
},
None => None,
};
let base = forecast_growth.unwrap_or(0.03);
let terminal = dcf::calculate_auto_terminal_growth(
wacc_comp.wacc,
fin.risk_free_rate,
fin.free_cash_flow,
Some(fin.revenue_history.first().copied().unwrap_or(0.0)),
forecast_growth,
);
JpDcfParams {
wacc: wacc_comp.wacc,
growth_rate_base: (base * 1000.0).round() / 1000.0,
growth_rate_bull: ((base + 0.04) * 1000.0).round() / 1000.0,
growth_rate_bear: ((base - 0.05).max(-0.05) * 1000.0).round() / 1000.0,
terminal_growth_rate: terminal,
reasoning_ja: format!(
"日本株版トライアルの自走算出（LLM未連携）。WACCはCAPM自走算出 {:.1}%（無リスク金利はJGB想定の暫定値 {:.1}%、βはデフォルト）。基本成長率はJ-Quants財務サマリーの会社予想（売上）から推定した {:.1}%。財務詳細はEDINET XBRL由来（取得できない場合はサマリー近似）。",
wacc_comp.wacc * 100.0,
JP_RISK_FREE_RATE * 100.0,
base * 100.0,
),
}
}

/// Fetch data, run the DCF, persist the theoretical price, and (when a bar
/// is available) today's daily price. Returns the watchlist row, the new
/// theoretical price row, and the bar used.
pub async fn calculate_jp_theoretical(
state: &AppState,
code: &str,
target_discount_rate: f64,
trigger_type: &str,
) -> anyhow::Result<(
JpWatchlistRow,
JpTheoreticalPriceRow,
Option<JpDailyBar>,
)> {
let listed = state.jquants.fetch_listed_info(code).await?;
let company_name = listed
.map(|l| l.company_name)
.filter(|n|!n.is_empty())
.unwrap_or_else(|| code.to_string());

let existing = db_jp::get_jp_watchlist(&state.pool, code).await?;
let watchlist = db_jp::upsert_jp_watchlist(
&state.pool,
code,
&company_name,
existing.as_ref().and_then(|w| w.edinet_code.as_deref()),
target_discount_rate,
)
.await?;

let (fin, summary, bar) =
build_jp_financials(state, code, watchlist.edinet_code.as_deref()).await?;
let params = derive_jp_params(&fin, summary.as_ref());
let out = dcf::calculate(&DcfInput {
cash_and_equivalents: fin.cash_and_short_term_investments,
total_debt: fin.total_debt,
free_cash_flow: fin.free_cash_flow,
shares_outstanding: fin.shares_outstanding,
wacc: params.wacc,
growth_rate_base: params.growth_rate_base,
growth_rate_bull: params.growth_rate_bull,
growth_rate_bear: params.growth_rate_bear,
terminal_growth_rate: params.terminal_growth_rate,
});
let tp = db_jp::insert_jp_theoretical_price(
&state.pool,
code,
trigger_type,
&out,
fin.free_cash_flow,
fin.shares_outstanding,
params.wacc,
params.growth_rate_base,
params.growth_rate_bull,
params.growth_rate_bear,
params.terminal_growth_rate,
&params.reasoning_ja,
)
.await?;

if let Some(b) = &bar {
let div = dcf::calculate_divergence_rate(b.close, tp.base_price);
db_jp::upsert_jp_daily_price(&state.pool, b, div).await?;
}
if let Some(d) = fin.latest_earnings_date {
db_jp::update_jp_last_earnings_date(&state.pool, code, d).await?;
}
Ok((watchlist, tp, bar))
}

// ---- HTTP handlers ----

pub async fn get_jp_watchlist(
State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<JpWatchlistItemResponse>>> {
let watchlists = db_jp::get_all_jp_watchlists(&state.pool)
.await
.map_err(internal_err)?;
let mut items = Vec::with_capacity(watchlists.len());
for w in &watchlists {
if let Some(tp) = db_jp::get_latest_jp_theoretical_price(&state.pool, &w.code)
.await
.map_err(internal_err)?
{
let dp = db_jp::get_latest_jp_daily_price(&state.pool, &w.code)
.await
.map_err(internal_err)?;
items.push(db_jp::build_jp_watchlist_item(w, &tp, dp.as_ref()));
}
}
Ok(Json(items))
}

pub async fn add_jp_watchlist(
State(state): State<Arc<AppState>>,
Json(req): Json<AddJpWatchlistRequest>,
) -> ApiResult<Json<JpWatchlistItemResponse>> {
let code = normalize_code(&req.code)
.ok_or_else(|| bad_request("code must be 1-8 alphanumeric characters (e.g. 7203)"))?;
let discount_rate = if req.target_discount_rate.is_finite() {
req.target_discount_rate.clamp(-50.0, 80.0)
} else {
20.0
};
let (watchlist, tp, _bar) =
calculate_jp_theoretical(&state, &code, discount_rate, "INITIAL")
.await
.map_err(internal_err)?;
let msg = format!(
"📊{} ({}) 理論株価(Base): ¥{:.2}",
watchlist.company_name, code, tp.base_price
);
db_jp::insert_jp_alert_log(&state.pool, &code, "INITIAL_CALC", &msg)
.await
.map_err(internal_err)?;
let dp = db_jp::get_latest_jp_daily_price(&state.pool, &code)
.await
.map_err(internal_err)?;
Ok(Json(db_jp::build_jp_watchlist_item(
&watchlist,
&tp,
dp.as_ref(),
)))
}

pub async fn delete_jp_watchlist(
State(state): State<Arc<AppState>>,
Path(code): Path<String>,
) -> ApiResult<impl IntoResponse> {
let code = normalize_code(&code).ok_or_else(|| bad_request("invalid code"))?;
let deleted = db_jp::delete_jp_watchlist(&state.pool, &code)
.await
.map_err(internal_err)?;
if deleted {
Ok(Json(json!({ "status": "deleted", "code": code})))
} else {
Err(not_found_err("JP watchlist entry not found"))
}
}

pub async fn get_jp_stock_detail(
State(state): State<Arc<AppState>>,
Path(code): Path<String>,
) -> ApiResult<Json<JpStockDetailResponse>> {
let code = normalize_code(&code).ok_or_else(|| bad_request("invalid code"))?;
let watchlist = db_jp::get_jp_watchlist(&state.pool, &code)
.await
.map_err(internal_err)?
.ok_or_else(|| not_found_err("JP watchlist entry not found"))?;
let latest_theoretical = db_jp::get_latest_jp_theoretical_price(&state.pool, &code)
.await
.map_err(internal_err)?;
let history = db_jp::get_jp_theoretical_history(&state.pool, &code)
.await
.map_err(internal_err)?;
let daily = db_jp::get_jp_daily_prices(&state.pool, &code)
.await
.map_err(internal_err)?;
let alerts = db_jp::get_jp_alerts_by_code(&state.pool, &code)
.await
.map_err(internal_err)?;
let (current_price, divergence_rate, is_undervalued_alert) = match (&latest_theoretical, daily.first()) {
(Some(tp), Some(dp)) => (
dp.close_price,
dp.divergence_rate,
dcf::is_undervalued(dp.close_price, tp.base_price, watchlist.target_discount_rate),
),
_ => (0.0, 0.0, false),
};
Ok(Json(JpStockDetailResponse {
watchlist,
current_price,
divergence_rate,
is_undervalued_alert,
latest_theoretical,
theoretical_history: history,
daily_prices: daily,
recent_alerts: alerts,
}))
}

pub async fn run_jp_daily_check_handler(
State(state): State<Arc<AppState>>,
) -> ApiResult<Json<JobSummaryResponse>> {
let summary = execute_jp_daily_check(&state).await.map_err(internal_err)?;
Ok(Json(summary))
}

/// JP daily-check: refresh the latest bar for every JP watchlist entry,
/// store it with its divergence rate, and record undervaluation alerts.
/// No-op when jp_watchlists is empty. LINE delivery is intentionally not
/// wired for JP in the trial; alerts are recorded in jp_alert_logs.
pub async fn execute_jp_daily_check(state: &AppState) -> anyhow::Result<JobSummaryResponse> {
let watchlists = db_jp::get_all_jp_watchlists(&state.pool).await?;
let mut processed = 0usize;
let mut alerts = 0usize;
let mut messages = Vec::new();

if watchlists.is_empty() {
messages.push("jp_watchlists is empty; JP daily-check is a no-op".to_string());
return Ok(JobSummaryResponse {
job_name: "jp-daily-check".to_string(),
processed_count: 0,
alerts_triggered: 0,
messages,
executed_at: Utc::now(),
});
}

for w in &watchlists {
let tp = match db_jp::get_latest_jp_theoretical_price(&state.pool, &w.code).await? {
Some(tp) => tp,
None => {
// First run for this code: calculate the theoretical price.
let (_, tp, _) =
calculate_jp_theoretical(state, &w.code, w.target_discount_rate, "INITIAL")
.await?;
tp
}
};
let bar = match state.jquants.fetch_latest_bar(&w.code).await {
Ok(Some(b)) => b,
Ok(None) => {
messages.push(format!("{}: no daily bar available; skipped", w.code));
continue;
}
Err(err) => {
messages.push(format!("{}: J-Quants bar fetch failed: {err}", w.code));
continue;
}
};
let div = dcf::calculate_divergence_rate(bar.close, tp.base_price);
db_jp::upsert_jp_daily_price(&state.pool, &bar, div).await?;
processed += 1;
messages.push(format!(
"{} ({}) close ¥{:.2} vs base ¥{:.2} ({:+.2}%)",
w.code, w.company_name, bar.close, tp.base_price, div
));
if dcf::is_undervalued(bar.close, tp.base_price, w.target_discount_rate) {
let msg = format!(
"🚨{} ({}) が理論株価(¥{:.2})から{:.2}%割安の ¥{:.2} に到達しました！（目標割安率: {:.0}%）",
w.company_name,
w.code,
tp.base_price,
div.abs(),
bar.close,
w.target_discount_rate
);
db_jp::insert_jp_alert_log(&state.pool, &w.code, "UNDERVALUED", &msg).await?;
alerts += 1;
messages.push(msg);
}
}

Ok(JobSummaryResponse {
job_name: "jp-daily-check".to_string(),
processed_count: processed,
alerts_triggered: alerts,
messages,
executed_at: Utc::now(),
})
}
