use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tracing::error;

use crate::{
    config::AppConfig,
    db,
    dcf::{self, DcfInput},
    models::{
        AddWatchlistRequest, AlertLogRow, JobSummaryResponse, RecalculateRequest,
        StockDetailResponse, UpdateWatchlistRequest, WatchlistItemResponse,
    },
    services::{
        fmp::FmpClient,
        line::LineClient,
        llm::{LlmClient, LlmDcfParams},
    },
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: AppConfig,
    pub fmp: FmpClient,
    pub llm: LlmClient,
    pub line: LineClient,
}

#[derive(Debug, Deserialize, Default)]
pub struct EarningsCheckQuery {
    pub force: Option<bool>,
}

type ApiResult<T> = Result<T, (StatusCode, Json<serde_json::Value>)>;

fn internal_err<E: std::fmt::Display>(err: E) -> (StatusCode, Json<serde_json::Value>) {
    error!("API error: {}", err);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": err.to_string() })),
    )
}

fn not_found_err(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg })))
}

pub async fn get_watchlist(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<WatchlistItemResponse>>> {
    let watchlists = db::get_all_watchlists(&state.pool)
        .await
        .map_err(internal_err)?;

    let mut items = Vec::with_capacity(watchlists.len());
    for w in &watchlists {
        if let Some(item) = db::build_watchlist_item_response(&state.pool, w)
            .await
            .map_err(internal_err)?
        {
            items.push(item);
        }
    }

    Ok(Json(items))
}

pub async fn add_watchlist(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddWatchlistRequest>,
) -> ApiResult<Json<WatchlistItemResponse>> {
    let symbol = req.ticker.trim().to_uppercase();
    if symbol.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Ticker symbol must not be empty" })),
        ));
    }

    let discount_rate = if req.target_discount_rate.is_finite() {
        req.target_discount_rate.clamp(-50.0, 80.0)
    } else {
        20.0
    };

    let (fin, news) = tokio::join!(
        state.fmp.fetch_financials(&symbol),
        state.fmp.fetch_news(&symbol)
    );

    let watch_row = db::upsert_watchlist(
        &state.pool,
        &fin.ticker,
        &fin.company_name,
        discount_rate,
        fin.latest_earnings_date,
    )
    .await
    .map_err(internal_err)?;

    let params = state.llm.infer_dcf_parameters(&fin, &news).await;
    let dcf_input = DcfInput {
        cash_and_equivalents: fin.cash_and_short_term_investments,
        total_debt: fin.total_debt,
        free_cash_flow: fin.free_cash_flow,
        shares_outstanding: fin.shares_outstanding,
        wacc: params.wacc,
        growth_rate_base: params.growth_rate_base,
        growth_rate_bull: params.growth_rate_bull,
        growth_rate_bear: params.growth_rate_bear,
        terminal_growth_rate: params.terminal_growth_rate,
    };
    let dcf_out = dcf::calculate(&dcf_input);

    db::insert_theoretical_price(
        &state.pool,
        &fin.ticker,
        "INITIAL",
        &dcf_out,
        fin.free_cash_flow,
        fin.shares_outstanding,
        &params,
    )
    .await
    .map_err(internal_err)?;

    let today = Utc::now().date_naive();
    let div_rate = dcf::calculate_divergence_rate(fin.current_price, dcf_out.base_price);
    db::upsert_daily_price(&state.pool, &fin.ticker, today, fin.current_price, div_rate)
        .await
        .map_err(internal_err)?;

    let msg = format!(
        "📊【新規銘柄登録＆理論株価算出】\n銘柄: {} ({})\n現在株価: ${:.2}\n理論株価(Base): ${:.2} (乖離率: {:+.2}%)\n強気(Bull): ${:.2} / 弱気(Bear): ${:.2}\nWACC(自走算出): {:.1}% / 基本成長率: {:.1}% / 永久成長率: {:.1}%\n\n📝 推論サマリー:\n{}",
        fin.company_name,
        fin.ticker,
        fin.current_price,
        dcf_out.base_price,
        div_rate,
        dcf_out.bull_price,
        dcf_out.bear_price,
        params.wacc * 100.0,
        params.growth_rate_base * 100.0,
        params.terminal_growth_rate * 100.0,
        params.reasoning_ja
    );

    state
        .line
        .send_and_log(&state.pool, &fin.ticker, "INITIAL_CALC", &msg)
        .await
        .map_err(internal_err)?;

    if dcf::is_undervalued(fin.current_price, dcf_out.base_price, discount_rate) {
        let alert_msg = format!(
            "🚨【割安アラート】{} が理論株価(${:.2})から{:.2}%割安の ${:.2} に到達しました！（目標割安率: {:.0}%）",
            fin.ticker,
            dcf_out.base_price,
            div_rate.abs(),
            fin.current_price,
            discount_rate
        );
        state
            .line
            .send_and_log(&state.pool, &fin.ticker, "UNDERVALUED", &alert_msg)
            .await
            .map_err(internal_err)?;
    }

    let item = db::build_watchlist_item_response(&state.pool, &watch_row)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| not_found_err("Failed to load created stock item"))?;

    Ok(Json(item))
}

pub async fn update_watchlist_settings(
    State(state): State<Arc<AppState>>,
    Path(ticker): Path<String>,
    Json(req): Json<UpdateWatchlistRequest>,
) -> ApiResult<Json<WatchlistItemResponse>> {
    let symbol = ticker.trim().to_uppercase();
    let discount_rate = req.target_discount_rate.clamp(-50.0, 80.0);

    let updated_row = db::update_watchlist_discount_rate(&state.pool, &symbol, discount_rate)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| not_found_err(&format!("Ticker {} not found", symbol)))?;

    let item = db::build_watchlist_item_response(&state.pool, &updated_row)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| not_found_err("Failed to load updated watchlist item"))?;

    Ok(Json(item))
}

pub async fn delete_watchlist(
    State(state): State<Arc<AppState>>,
    Path(ticker): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let symbol = ticker.trim().to_uppercase();
    let deleted = db::delete_watchlist(&state.pool, &symbol)
        .await
        .map_err(internal_err)?;

    if !deleted {
        return Err(not_found_err(&format!("Ticker {} not found", symbol)));
    }

    Ok(Json(json!({ "deleted": true, "ticker": symbol })))
}

pub async fn get_stock_detail(
    State(state): State<Arc<AppState>>,
    Path(ticker): Path<String>,
) -> ApiResult<Json<StockDetailResponse>> {
    let symbol = ticker.trim().to_uppercase();
    let watchlist = db::get_watchlist_by_ticker(&state.pool, &symbol)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| not_found_err(&format!("Ticker {} not found", symbol)))?;

    let (latest_theoretical, theoretical_history, daily_prices, recent_alerts, fin) = tokio::join!(
        db::get_latest_theoretical_price(&state.pool, &symbol),
        db::get_theoretical_price_history(&state.pool, &symbol),
        db::get_daily_prices(&state.pool, &symbol),
        db::get_alerts_by_ticker(&state.pool, &symbol),
        state.fmp.fetch_financials(&symbol),
    );

    let latest_theoretical = latest_theoretical.map_err(internal_err)?;
    let theoretical_history = theoretical_history.map_err(internal_err)?;
    let daily_prices = daily_prices.map_err(internal_err)?;
    let recent_alerts = recent_alerts.map_err(internal_err)?;

    let base_price = latest_theoretical
        .as_ref()
        .map(|t| t.base_price)
        .unwrap_or(0.0);
    let current_price = daily_prices
        .first()
        .map(|d| d.close_price)
        .unwrap_or(fin.current_price);
    let divergence_rate = dcf::calculate_divergence_rate(current_price, base_price);
    let is_undervalued_alert =
        dcf::is_undervalued(current_price, base_price, watchlist.target_discount_rate);

    let auto_params = LlmClient::fallback_dcf_params(&fin);
    let latest_revenue = fin.revenue_history.first().copied().filter(|&r| r > 0.0);
    let fcf_for_margin = latest_theoretical
        .as_ref()
        .map(|t| t.free_cash_flow)
        .unwrap_or(fin.free_cash_flow);
    let fcf_margin = latest_revenue.and_then(|rev| {
        if rev > 0.0 && fcf_for_margin != 0.0 {
            Some(dcf::round_three_decimals(fcf_for_margin / rev))
        } else {
            None
        }
    });

    let implied_growth_rate = latest_theoretical.as_ref().and_then(|tp| {
        dcf::calculate_implied_growth_rate(
            current_price,
            tp.net_cash,
            0.0,
            tp.free_cash_flow,
            tp.shares_outstanding,
            tp.wacc,
            tp.terminal_growth_rate,
        )
    });

    Ok(Json(StockDetailResponse {
        watchlist,
        current_price,
        divergence_rate,
        is_undervalued_alert,
        implied_growth_rate,
        auto_wacc: Some(auto_params.wacc),
        auto_terminal_growth_rate: Some(auto_params.terminal_growth_rate),
        beta: fin.beta,
        risk_free_rate: fin.risk_free_rate,
        effective_tax_rate: fin.effective_tax_rate,
        latest_revenue,
        fcf_margin,
        latest_theoretical,
        theoretical_history,
        daily_prices,
        recent_alerts,
    }))
}

pub async fn recalculate_stock(
    State(state): State<Arc<AppState>>,
    Path(ticker): Path<String>,
    body: Bytes,
) -> ApiResult<Json<WatchlistItemResponse>> {
    let symbol = ticker.trim().to_uppercase();
    let req: RecalculateRequest = if body.is_empty() {
        RecalculateRequest::default()
    } else {
        serde_json::from_slice(&body).unwrap_or_default()
    };

    let mut watchlist = db::get_watchlist_by_ticker(&state.pool, &symbol)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| not_found_err(&format!("Ticker {} not found", symbol)))?;

    if let Some(new_discount_rate) = req.custom_target_discount_rate {
        if new_discount_rate.is_finite() {
            let clamped = new_discount_rate.clamp(-50.0, 80.0);
            if let Some(updated) =
                db::update_watchlist_discount_rate(&state.pool, &symbol, clamped)
                    .await
                    .map_err(internal_err)?
            {
                watchlist = updated;
            }
        }
    }

    let prev_tp = db::get_latest_theoretical_price(&state.pool, &symbol)
        .await
        .map_err(internal_err)?;

    let fin = state.fmp.fetch_financials(&symbol).await;

    let has_custom_override = req.custom_wacc.is_some()
        || req.custom_growth_rate_base.is_some()
        || req.custom_growth_rate_bull.is_some()
        || req.custom_growth_rate_bear.is_some()
        || req.custom_terminal_growth_rate.is_some();

    let params = if has_custom_override {
        let base_ref = match &prev_tp {
            Some(tp) => LlmDcfParams {
                wacc: tp.wacc,
                growth_rate_base: tp.growth_rate_base,
                growth_rate_bull: tp.growth_rate_bull,
                growth_rate_bear: tp.growth_rate_bear,
                terminal_growth_rate: tp.terminal_growth_rate,
                reasoning_ja: tp.reasoning_ja.clone(),
            },
            None => LlmClient::fallback_dcf_params(&fin),
        };

        let wacc = req.custom_wacc.unwrap_or(base_ref.wacc);
        let growth_rate_base = req
            .custom_growth_rate_base
            .unwrap_or(base_ref.growth_rate_base);
        let delta = growth_rate_base - base_ref.growth_rate_base;
        let growth_rate_bull = req
            .custom_growth_rate_bull
            .unwrap_or_else(|| (base_ref.growth_rate_bull + delta).max(growth_rate_base + 0.02));
        let growth_rate_bear = req
            .custom_growth_rate_bear
            .unwrap_or_else(|| {
                (base_ref.growth_rate_bear + delta)
                    .min(growth_rate_base - 0.02)
                    .max(0.0)
            });
        let terminal_growth_rate = req
            .custom_terminal_growth_rate
            .unwrap_or(base_ref.terminal_growth_rate);

        // Strip previous manual header if present so it doesn't stack infinitely
        let clean_prev_reasoning = if let Some(idx) = base_ref.reasoning_ja.find("【Yahoo Finance")
        {
            &base_ref.reasoning_ja[idx..]
        } else if let Some(idx) = base_ref.reasoning_ja.find("【推論レポート】") {
            &base_ref.reasoning_ja[idx..]
        } else {
            base_ref.reasoning_ja.as_str()
        };

        LlmDcfParams {
            wacc,
            growth_rate_base,
            growth_rate_bull,
            growth_rate_bear,
            terminal_growth_rate,
            reasoning_ja: format!(
                "【手動カスタム前提シミュレーション (WACC: {:.1}%, 基本成長率: {:.1}%, 永久成長率: {:.1}%)】\n{}",
                wacc * 100.0,
                growth_rate_base * 100.0,
                terminal_growth_rate * 100.0,
                clean_prev_reasoning
            ),
        }
    } else {
        // 画面からの「自走算出値にリセット」操作ではLLMトークンを消費せず、プログラム側のCAPM＆財務推論を使用
        LlmClient::fallback_dcf_params(&fin)
    };

    let dcf_input = DcfInput {
        cash_and_equivalents: fin.cash_and_short_term_investments,
        total_debt: fin.total_debt,
        free_cash_flow: fin.free_cash_flow,
        shares_outstanding: fin.shares_outstanding,
        wacc: params.wacc,
        growth_rate_base: params.growth_rate_base,
        growth_rate_bull: params.growth_rate_bull,
        growth_rate_bear: params.growth_rate_bear,
        terminal_growth_rate: params.terminal_growth_rate,
    };
    let dcf_out = dcf::calculate(&dcf_input);

    db::insert_theoretical_price(
        &state.pool,
        &symbol,
        "MANUAL",
        &dcf_out,
        fin.free_cash_flow,
        fin.shares_outstanding,
        &params,
    )
    .await
    .map_err(internal_err)?;

    let today = Utc::now().date_naive();
    let div_rate = dcf::calculate_divergence_rate(fin.current_price, dcf_out.base_price);
    db::upsert_daily_price(&state.pool, &symbol, today, fin.current_price, div_rate)
        .await
        .map_err(internal_err)?;

    let item = db::build_watchlist_item_response(&state.pool, &watchlist)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| not_found_err("Failed to load recalculated stock"))?;

    Ok(Json(item))
}

pub async fn run_daily_check_handler(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<JobSummaryResponse>> {
    let summary = execute_daily_check(&state).await.map_err(internal_err)?;
    Ok(Json(summary))
}

pub async fn run_earnings_check_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<EarningsCheckQuery>,
) -> ApiResult<Json<JobSummaryResponse>> {
    let force = query.force.unwrap_or(false);
    let summary = execute_earnings_check(&state, force)
        .await
        .map_err(internal_err)?;
    Ok(Json(summary))
}

#[derive(serde::Serialize)]
pub struct SystemInfoResponse {
    pub llm_model: String,
    pub llm_news_model: String,
}

pub async fn get_system_info(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<SystemInfoResponse>> {
    Ok(Json(SystemInfoResponse {
        llm_model: state.config.llm_model.clone(),
        llm_news_model: state.config.llm_news_model.clone(),
    }))
}

pub async fn get_alerts(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<AlertLogRow>>> {
    let alerts = db::get_recent_alerts(&state.pool, 30)
        .await
        .map_err(internal_err)?;
    Ok(Json(alerts))
}

pub async fn execute_daily_check(state: &AppState) -> anyhow::Result<JobSummaryResponse> {
    let watchlists = db::get_all_watchlists(&state.pool).await?;
    let today = Utc::now().date_naive();

    let mut alerts_triggered = 0usize;
    let mut messages = Vec::new();

    for w in &watchlists {
        let fin = state.fmp.fetch_financials(&w.ticker).await;
        let Some(tp) = db::get_latest_theoretical_price(&state.pool, &w.ticker).await? else {
            continue;
        };

        let div_rate = dcf::calculate_divergence_rate(fin.current_price, tp.base_price);
        db::upsert_daily_price(&state.pool, &w.ticker, today, fin.current_price, div_rate).await?;

        if dcf::is_undervalued(fin.current_price, tp.base_price, w.target_discount_rate) {
            let alert_msg = format!(
                "🚨【割安アラート】{} が理論株価({:.2}$)から{:.2}%割安の {:.2}$ に到達しました！（目標割安率: {:.0}%）",
                w.ticker,
                tp.base_price,
                div_rate.abs(),
                fin.current_price,
                w.target_discount_rate
            );
            state
                .line
                .send_and_log(&state.pool, &w.ticker, "UNDERVALUED", &alert_msg)
                .await?;
            alerts_triggered += 1;
            messages.push(alert_msg);
        }

        let news = state.fmp.fetch_news(&w.ticker).await;
        let summary_ja = state
            .llm
            .summarize_news(&w.ticker, &w.company_name, &news)
            .await;

        let news_msg = format!(
            "📰【モーニングニュース要約】{} ({})\n現在株価: ${:.2} / 理論株価(Base): ${:.2} ({:+.2}%)\n\n{}",
            w.company_name, w.ticker, fin.current_price, tp.base_price, div_rate, summary_ja
        );
        state
            .line
            .send_and_log(&state.pool, &w.ticker, "NEWS_SUMMARY", &news_msg)
            .await?;
        alerts_triggered += 1;
        messages.push(news_msg);
    }

    Ok(JobSummaryResponse {
        job_name: "daily-check".to_string(),
        processed_count: watchlists.len(),
        alerts_triggered,
        messages,
        executed_at: Utc::now(),
    })
}

pub async fn execute_earnings_check(
    state: &AppState,
    force: bool,
) -> anyhow::Result<JobSummaryResponse> {
    let watchlists = db::get_all_watchlists(&state.pool).await?;
    let today = Utc::now().date_naive();

    let semaphore = Arc::new(tokio::sync::Semaphore::new(6));
    let mut join_set = tokio::task::JoinSet::new();

    for (idx, w) in watchlists.iter().cloned().enumerate() {
        let sem = semaphore.clone();
        let pool = state.pool.clone();
        let fmp = state.fmp.clone();
        let llm = state.llm.clone();
        let line = state.line.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire().await.ok();
            let fin = fmp.fetch_financials(&w.ticker).await;
            let is_new_earnings = match (fin.latest_earnings_date, w.last_earnings_date) {
                (Some(latest), Some(prev)) => latest > prev,
                (Some(_), None) => true,
                _ => false,
            };

            if !is_new_earnings && !force {
                return Ok::<Option<(usize, String)>, anyhow::Error>(None);
            }

            let prev_tp = db::get_latest_theoretical_price(&pool, &w.ticker).await?;
            let old_base = prev_tp.as_ref().map(|t| t.base_price).unwrap_or(0.0);

            let news = fmp.fetch_news(&w.ticker).await;
            let params = llm.infer_dcf_parameters(&fin, &news).await;

            let dcf_input = DcfInput {
                cash_and_equivalents: fin.cash_and_short_term_investments,
                total_debt: fin.total_debt,
                free_cash_flow: fin.free_cash_flow,
                shares_outstanding: fin.shares_outstanding,
                wacc: params.wacc,
                growth_rate_base: params.growth_rate_base,
                growth_rate_bull: params.growth_rate_bull,
                growth_rate_bear: params.growth_rate_bear,
                terminal_growth_rate: params.terminal_growth_rate,
            };
            let dcf_out = dcf::calculate(&dcf_input);

            db::insert_theoretical_price(
                &pool,
                &w.ticker,
                "EARNINGS_UPDATE",
                &dcf_out,
                fin.free_cash_flow,
                fin.shares_outstanding,
                &params,
            )
            .await?;

            if let Some(earnings_date) = fin.latest_earnings_date {
                db::update_watchlist_earnings_date(&pool, &w.ticker, earnings_date).await?;
            }

            let div_rate = dcf::calculate_divergence_rate(fin.current_price, dcf_out.base_price);
            db::upsert_daily_price(&pool, &w.ticker, today, fin.current_price, div_rate).await?;

            // Build earnings metadata lines ONLY from fields actually obtained from Yahoo Finance / FMP
            let mut meta_lines = Vec::new();
            if let Some(ed) = fin.latest_earnings_date {
                meta_lines.push(format!("直近決算期末日: {}", ed));
            }
            match (fin.eps_actual, fin.eps_estimated) {
                (Some(actual), Some(est)) if est > 0.0 => {
                    let beat_pct = ((actual - est) / est) * 100.0;
                    meta_lines.push(format!(
                        "EPS実績: ${:.2} vs 予想: ${:.2} (サプライズ: {:+.1}%)",
                        actual, est, beat_pct
                    ));
                }
                (Some(actual), _) => {
                    meta_lines.push(format!("直近四半期希薄化後EPS実績: ${:.2}", actual));
                }
                _ => {}
            }

            let meta_section = if meta_lines.is_empty() {
                String::new()
            } else {
                format!("{}\n", meta_lines.join("\n"))
            };

            let price_diff = dcf_out.base_price - old_base;
            let earnings_msg = format!(
                "📊【四半期決算＆理論株価アップデート】{} ({})\n{}\n🔄 理論株価の再計算結果 (WACC {:.1}% / 永久成長率 {:.1}%):\n旧Base理論株価: ${:.2} ➔ 新Base理論株価: ${:.2} ({:+.2}$)\n新Bull: ${:.2} / 新Bear: ${:.2}\n現在株価: ${:.2} (新乖離率: {:+.2}%)",
                w.company_name,
                w.ticker,
                meta_section,
                params.wacc * 100.0,
                params.terminal_growth_rate * 100.0,
                old_base,
                dcf_out.base_price,
                price_diff,
                dcf_out.bull_price,
                dcf_out.bear_price,
                fin.current_price,
                div_rate
            );

            line.send_and_log(&pool, &w.ticker, "EARNINGS", &earnings_msg)
                .await?;

            Ok(Some((idx, earnings_msg)))
        });
    }

    let mut ordered_results = Vec::new();
    while let Some(res) = join_set.join_next().await {
        if let Some(pair) = res?? {
            ordered_results.push(pair);
        }
    }
    ordered_results.sort_by_key(|(idx, _)| *idx);
    let alerts_triggered = ordered_results.len();
    let messages = ordered_results.into_iter().map(|(_, m)| m).collect();

    Ok(JobSummaryResponse {
        job_name: "earnings-check".to_string(),
        processed_count: watchlists.len(),
        alerts_triggered,
        messages,
        executed_at: Utc::now(),
    })
}
