//! J-Quants API (JPX) client for the Japanese-stock version.
//!
//! - API V2: base `https://api.jquants.com/v2`, auth via `x-api-key` header
//!   (env `JQUANTS_API_KEY`). V1 is deprecated; new registrations are V2-only.
//! - When the key is unset/mock, the client returns clearly-labelled MOCK
//!   data (7203 Toyota-shaped figures) so the whole JP flow runs end to end
//!   before registration is completed. This mirrors the FMP mock fallback.
//!
//! Endpoint notes (checked against the official V2 spec on 2026-11-11):
//! - Daily bars:        GET /equities/bars/daily?code=&from=&to=
//! - Listed master:     GET /equities/master?code= (or ?date=)
//! - Financial summary: GET /fins/summary?code= (or ?date=)
//! - Earnings schedule: GET /markets/earnings-calendar — CAUTION: the exact
//!   V2 path for the earnings-announcement schedule was not fully confirmed
//!   in the public spec excerpts available at implementation time. Verify
//!   against the official V2 reference before relying on it; failures are
//!   treated as "no schedule data" instead of failing the batch.

use chrono::NaiveDate;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use crate::config::AppConfig;

const JQUANTS_BASE: &str = "https://api.jquants.com/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JpDailyBar {
    pub code: String,
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JpListedInfo {
    pub code: String,
    pub company_name: String,
    pub market: Option<String>,
    pub sector: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JpFinSummary {
    pub code: String,
    pub disclosed_date: Option<NaiveDate>,
    pub revenue: Option<f64>,
    pub operating_profit: Option<f64>,
    pub ordinary_profit: Option<f64>,
    pub net_income: Option<f64>,
    pub eps: Option<f64>,
    /// Company-issued forecasts (決算短信の業績予想) — the JP replacement for
    /// analyst estimates.
    pub forecast_revenue: Option<f64>,
    pub forecast_eps: Option<f64>,
    pub net_assets: Option<f64>,
    pub total_assets: Option<f64>,
    pub equity_ratio: Option<f64>,
    pub shares_outstanding: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JpEarningsSchedule {
    pub code: String,
    pub scheduled_date: NaiveDate,
}

#[derive(Debug, Clone)]
pub struct JQuantsClient {
    http: Client,
    config: AppConfig,
}

impl JQuantsClient {
    pub fn new(config: AppConfig) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            config,
        }
    }

    pub fn is_mock(&self) -> bool {
        self.config.is_jquants_mock()
    }

    async fn get_json(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<Value> {
        let url = format!("{JQUANTS_BASE}{path}");
        let resp = self
            .http
            .get(&url)
            .header("x-api-key", &self.config.jquants_api_key)
            .query(query)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("J-Quants API {path} returned HTTP {}", resp.status());
        }
        Ok(resp.json().await?)
    }

    /// Daily OHLCV bars for `code` in [from, to]. Mock mode returns a small
    /// MOCK series ending at the latest verified Toyota close (¥3,435 on
    /// 2026-11-11).
    pub async fn fetch_daily_bars(
        &self,
        code: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<Vec<JpDailyBar>> {
        let code = code.trim().to_string();
        if self.is_mock() {
            info!("JQUANTS_MOCK: returning mock daily bars for {code}");
            return Ok(Self::mock_daily_bars(&code));
        }
        let v = self
            .get_json(
                "/equities/bars/daily",
                &[
                    ("code", code.clone()),
                    ("from", from.format("%Y-%m-%d").to_string()),
                    ("to", to.format("%Y-%m-%d").to_string()),
                ],
            )
            .await?;
        let mut bars: Vec<JpDailyBar> = rows_of(&v)
            .iter()
            .filter_map(|r| parse_bar(r, &code))
            .collect();
        bars.sort_by_key(|b| b.date);
        Ok(bars)
    }

    pub async fn fetch_latest_bar(&self, code: &str) -> anyhow::Result<Option<JpDailyBar>> {
        let today = chrono::Utc::now().date_naive();
        let from = today - chrono::Duration::days(14);
        let bars = self.fetch_daily_bars(code, from, today).await?;
        Ok(bars.into_iter().max_by_key(|b| b.date))
    }

    pub async fn fetch_listed_info(&self, code: &str) -> anyhow::Result<Option<JpListedInfo>> {
        let code = code.trim().to_string();
        if self.is_mock() {
            info!("JQUANTS_MOCK: returning mock listed info for {code}");
            return Ok(Some(Self::mock_listed_info(&code)));
        }
        let v = self
            .get_json("/equities/master", &[("code", code.clone())])
            .await?;
        Ok(rows_of(&v).first().map(|r| JpListedInfo {
            code: str_of(r, &["Code", "LocalCode"]).unwrap_or(code),
            company_name: str_of(r, &["CompanyName", "CoName", "Name"]).unwrap_or_default(),
            market: str_of(r, &["MarketCodeName", "Market"]),
            sector: str_of(r, &["Sector33CodeName", "Sector17CodeName", "Sector"]),
        }))
    }

    /// Latest financial summary (決算短信サマリー) for `code`, including the
    /// company-issued forecast fields.
    pub async fn fetch_latest_summary(&self, code: &str) -> anyhow::Result<Option<JpFinSummary>> {
        let code = code.trim().to_string();
        if self.is_mock() {
            info!("JQUANTS_MOCK: returning mock financial summary for {code}");
            return Ok(Some(Self::mock_summary(&code)));
        }
        let v = self
            .get_json("/fins/summary", &[("code", code.clone())])
            .await?;
        let mut summaries: Vec<JpFinSummary> =
            rows_of(&v).iter().map(|r| parse_summary(r, &code)).collect();
        summaries.sort_by_key(|s| s.disclosed_date);
        Ok(summaries.pop())
    }

    /// Upcoming earnings announcement dates.
    /// NOTE: the exact V2 path is unverified (see module docs); any failure
    /// yields an empty list by design.
    pub async fn fetch_earnings_schedule(
        &self,
        code: &str,
    ) -> anyhow::Result<Vec<JpEarningsSchedule>> {
        let code = code.trim().to_string();
        if self.is_mock() {
            info!("JQUANTS_MOCK: returning mock earnings schedule for {code}");
            return Ok(vec![]);
        }
        match self
            .get_json("/markets/earnings-calendar", &[("code", code.clone())])
            .await
        {
            Ok(v) => Ok(rows_of(&v)
                .iter()
                .filter_map(|r| {
                    let d = str_of(r, &["Date", "EarningsDate", "ScheduledDate"])?;
                    NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok().map(|date| {
                        JpEarningsSchedule {
                            code: code.clone(),
                            scheduled_date: date,
                        }
                    })
                })
                .collect()),
            Err(err) => {
                warn!("J-Quants earnings schedule unavailable for {code}: {err}");
                Ok(vec![])
            }
        }
    }

    // ---- MOCK data (clearly labelled; figures are Toyota-shaped, JPY) ----

    fn mock_listed_info(code: &str) -> JpListedInfo {
        let (name, market, sector) = match code {
            "7203" => ("トヨタ自動車", "プライム", "輸送用機器"),
            "6758" => ("ソニーグループ", "プライム", "電気機器"),
            "8306" => ("三菱UFJフィナンシャル・グループ", "プライム", "銀行業"),
            _ => ("モック銘柄", "プライム", "その他"),
        };
        JpListedInfo {
            code: code.to_string(),
            company_name: name.to_string(),
            market: Some(market.to_string()),
            sector: Some(sector.to_string()),
        }
    }

    fn mock_daily_bars(code: &str) -> Vec<JpDailyBar> {
        // Real verified closes for 7203 around the mock build date; other
        // codes reuse the same shape so flows can be exercised.
        let raw: [(&str, f64, f64, f64, f64, f64); 3] = [
            ("2026-11-09", 3396.0, 3491.0, 3333.0, 3491.0, 36_132_200.0),
            ("2026-11-10", 3492.0, 3493.0, 3407.0, 3414.0, 28_932_000.0),
            ("2026-11-11", 3395.0, 3444.0, 3383.0, 3435.0, 25_465_200.0),
        ];
        raw.iter()
            .filter_map(|(d, o, h, l, c, v)| {
                NaiveDate::parse_from_str(d, "%Y-%m-%d").ok().map(|date| {
                    JpDailyBar {
                        code: code.to_string(),
                        date,
                        open: *o,
                        high: *h,
                        low: *l,
                        close: *c,
                        volume: *v,
                    }
                })
            })
            .collect()
    }

    fn mock_summary(code: &str) -> JpFinSummary {
        // Toyota FY2025-shaped summary values (JPY). MOCK — for flow testing
        // until a real J-Quants key is registered.
        JpFinSummary {
            code: code.to_string(),
            disclosed_date: NaiveDate::from_ymd_opt(2026, 11, 5),
            revenue: Some(48_036_700_000_000.0),
            operating_profit: Some(4_795_600_000_000.0),
            ordinary_profit: Some(5_245_700_000_000.0),
            net_income: Some(4_765_100_000_000.0),
            eps: Some(362.54),
            forecast_revenue: Some(48_500_000_000_000.0),
            forecast_eps: Some(370.0),
            net_assets: Some(35_200_000_000_000.0),
            total_assets: Some(93_500_000_000_000.0),
            equity_ratio: Some(0.376),
            shares_outstanding: Some(13_140_000_000.0),
        }
    }
}

// ---- loose JSON parsing helpers (V2 field names have minor variants) ----

fn rows_of(v: &Value) -> Vec<Value> {
    if let Value::Array(a) = v {
        return a.clone();
    }
    for key in ["data", "daily_quotes", "bars", "statements", "info", "results"] {
        if let Some(Value::Array(a)) = v.get(key) {
            return a.clone();
        }
    }
    vec![]
}

fn num_of(v: &Value, keys: &[&str]) -> Option<f64> {
    for k in keys {
        match v.get(k) {
            Some(Value::Number(n)) => {
                if let Some(f) = n.as_f64() {
                    return Some(f);
                }
            }
            Some(Value::String(s)) => {
                if let Ok(f) = s.trim().parse::<f64>() {
                    return Some(f);
                }
            }
            _ => {}
        }
    }
    None
}

fn str_of(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(Value::String(s)) = v.get(k) {
            if !s.is_empty() {
                return Some(s.clone());
            }
        }
    }
    None
}

fn parse_bar(r: &Value, fallback_code: &str) -> Option<JpDailyBar> {
    let date_s = str_of(r, &["Date", "TradeDate"])?;
    let date = NaiveDate::parse_from_str(&date_s, "%Y-%m-%d").ok()?;
    Some(JpDailyBar {
        code: str_of(r, &["Code", "LocalCode"]).unwrap_or_else(|| fallback_code.to_string()),
        date,
        open: num_of(r, &["Open", "O", "AdjustmentOpen"])?,
        high: num_of(r, &["High", "H", "AdjustmentHigh"])?,
        low: num_of(r, &["Low", "L", "AdjustmentLow"])?,
        close: num_of(r, &["Close", "C", "AdjClose", "AdjustmentClose"])?,
        volume: num_of(r, &["Volume", "Vo", "TurnoverVolume"]).unwrap_or(0.0),
    })
}

fn parse_summary(r: &Value, fallback_code: &str) -> JpFinSummary {
    let disclosed = str_of(r, &["DisclosedDate", "DisclosureDate", "Date"])
        .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    JpFinSummary {
        code: str_of(r, &["LocalCode", "Code"]).unwrap_or_else(|| fallback_code.to_string()),
        disclosed_date: disclosed,
        revenue: num_of(r, &["NetSales", "Revenue", "Sales"]),
        operating_profit: num_of(r, &["OperatingProfit"]),
        ordinary_profit: num_of(r, &["OrdinaryProfit"]),
        net_income: num_of(r, &["Profit", "NetIncome"]),
        eps: num_of(r, &["EarningsPerShare", "EPS"]),
        forecast_revenue: num_of(r, &["ForecastNetSales", "ForecastRevenue"]),
        forecast_eps: num_of(r, &["ForecastEarningsPerShare", "ForecastEPS"]),
        net_assets: num_of(r, &["Equity", "NetAssets"]),
        total_assets: num_of(r, &["TotalAssets"]),
        equity_ratio: num_of(r, &["EquityToTotalAssetRatio", "EquityRatio"]),
        shares_outstanding: num_of(r, &[
            "NumberOfIssuedAndOutstandingSharesAtTheEndOfFiscalYearIncludingTreasuryStock",
            "SharesOutstanding",
        ]),
    }
}
