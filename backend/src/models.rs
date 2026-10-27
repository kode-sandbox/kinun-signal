use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WatchlistRow {
    pub ticker: String,
    pub company_name: String,
    pub target_discount_rate: f64,
    pub last_earnings_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TheoreticalPriceRow {
    pub id: i32,
    pub ticker: String,
    pub trigger_type: String,
    pub base_price: f64,
    pub bull_price: f64,
    pub bear_price: f64,
    pub net_cash: f64,
    pub free_cash_flow: f64,
    pub shares_outstanding: f64,
    pub wacc: f64,
    pub growth_rate_base: f64,
    pub growth_rate_bull: f64,
    pub growth_rate_bear: f64,
    pub terminal_growth_rate: f64,
    pub reasoning_ja: String,
    pub calculated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DailyPriceRow {
    pub id: i32,
    pub ticker: String,
    pub trade_date: NaiveDate,
    pub close_price: f64,
    pub divergence_rate: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AlertLogRow {
    pub id: i32,
    pub ticker: String,
    pub alert_type: String,
    pub message: String,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistItemResponse {
    pub ticker: String,
    pub company_name: String,
    pub target_discount_rate: f64,
    pub current_price: f64,
    pub base_price: f64,
    pub bull_price: f64,
    pub bear_price: f64,
    pub divergence_rate: f64,
    pub is_undervalued_alert: bool,
    pub wacc: f64,
    pub growth_rate_base: f64,
    pub growth_rate_bull: f64,
    pub growth_rate_bear: f64,
    pub terminal_growth_rate: f64,
    pub implied_growth_rate: Option<f64>,
    pub net_cash: f64,
    pub free_cash_flow: f64,
    pub shares_outstanding: f64,
    pub reasoning_ja: String,
    pub last_earnings_date: Option<NaiveDate>,
    pub calculated_at: DateTime<Utc>,
}

fn default_discount_rate() -> f64 {
    20.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddWatchlistRequest {
    pub ticker: String,
    #[serde(default = "default_discount_rate")]
    pub target_discount_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWatchlistRequest {
    pub target_discount_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockDetailResponse {
    pub watchlist: WatchlistRow,
    pub current_price: f64,
    pub divergence_rate: f64,
    pub is_undervalued_alert: bool,
    pub implied_growth_rate: Option<f64>,
    pub auto_wacc: Option<f64>,
    pub auto_terminal_growth_rate: Option<f64>,
    pub beta: Option<f64>,
    pub risk_free_rate: Option<f64>,
    pub effective_tax_rate: Option<f64>,
    pub latest_revenue: Option<f64>,
    pub fcf_margin: Option<f64>,
    pub latest_theoretical: Option<TheoreticalPriceRow>,
    pub theoretical_history: Vec<TheoreticalPriceRow>,
    pub daily_prices: Vec<DailyPriceRow>,
    pub recent_alerts: Vec<AlertLogRow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecalculateRequest {
    pub custom_wacc: Option<f64>,
    pub custom_growth_rate_base: Option<f64>,
    pub custom_growth_rate_bull: Option<f64>,
    pub custom_growth_rate_bear: Option<f64>,
    pub custom_terminal_growth_rate: Option<f64>,
    pub custom_target_discount_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummaryResponse {
    pub job_name: String,
    pub processed_count: usize,
    pub alerts_triggered: usize,
    pub messages: Vec<String>,
    pub executed_at: DateTime<Utc>,
}
