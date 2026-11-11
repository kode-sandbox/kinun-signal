//! DB access for the Japanese-stock tables (jp_*). Separate from db.rs so
//! the US tables and queries stay untouched.

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::{
    dcf::DcfOutput,
    models::{
        JpAlertLogRow, JpDailyPriceRow, JpTheoreticalPriceRow, JpWatchlistItemResponse,
        JpWatchlistRow,
    },
    services::jquants::JpDailyBar,
};

const JP_WATCHLIST_COLS: &str =
    "code, company_name, edinet_code, target_discount_rate, last_earnings_date, created_at, updated_at";
const JP_TP_COLS: &str = "id, code, trigger_type, base_price, bull_price, bear_price, net_cash, \
    free_cash_flow, shares_outstanding, wacc, growth_rate_base, growth_rate_bull, \
    growth_rate_bear, terminal_growth_rate, reasoning_ja, calculated_at";
const JP_DP_COLS: &str = "id, code, trade_date, open_price, high_price, low_price, close_price, \
    volume, divergence_rate, created_at";

pub async fn upsert_jp_watchlist(
    pool: &PgPool,
    code: &str,
    company_name: &str,
    edinet_code: Option<&str>,
    target_discount_rate: f64,
) -> anyhow::Result<JpWatchlistRow> {
    let row = sqlx::query_as::<_, JpWatchlistRow>(&format!(
        r#"
        INSERT INTO jp_watchlists (code, company_name, edinet_code, target_discount_rate, updated_at)
        VALUES ($1, $2, $3, $4, NOW())
        ON CONFLICT (code) DO UPDATE
        SET company_name = EXCLUDED.company_name,
            edinet_code = COALESCE(EXCLUDED.edinet_code, jp_watchlists.edinet_code),
            target_discount_rate = EXCLUDED.target_discount_rate,
            updated_at = NOW()
        RETURNING {JP_WATCHLIST_COLS}
        "#
    ))
    .bind(code)
    .bind(company_name)
    .bind(edinet_code)
    .bind(target_discount_rate)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_all_jp_watchlists(pool: &PgPool) -> anyhow::Result<Vec<JpWatchlistRow>> {
    let rows = sqlx::query_as::<_, JpWatchlistRow>(&format!(
        "SELECT {JP_WATCHLIST_COLS} FROM jp_watchlists ORDER BY code"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_jp_watchlist(pool: &PgPool, code: &str) -> anyhow::Result<Option<JpWatchlistRow>> {
    let row = sqlx::query_as::<_, JpWatchlistRow>(&format!(
        "SELECT {JP_WATCHLIST_COLS} FROM jp_watchlists WHERE code = $1"
    ))
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete_jp_watchlist(pool: &PgPool, code: &str) -> anyhow::Result<bool> {
    let res = sqlx::query("DELETE FROM jp_watchlists WHERE code = $1")
        .bind(code)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn update_jp_last_earnings_date(
    pool: &PgPool,
    code: &str,
    date: NaiveDate,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE jp_watchlists SET last_earnings_date = $2, updated_at = NOW() WHERE code = $1",
    )
    .bind(code)
    .bind(date)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_jp_theoretical_price(
    pool: &PgPool,
    code: &str,
    trigger_type: &str,
    dcf_out: &DcfOutput,
    free_cash_flow: f64,
    shares_outstanding: f64,
    wacc: f64,
    growth_rate_base: f64,
    growth_rate_bull: f64,
    growth_rate_bear: f64,
    terminal_growth_rate: f64,
    reasoning_ja: &str,
) -> anyhow::Result<JpTheoreticalPriceRow> {
    let row = sqlx::query_as::<_, JpTheoreticalPriceRow>(&format!(
        r#"
        INSERT INTO jp_theoretical_prices (
            code, trigger_type, base_price, bull_price, bear_price,
            net_cash, free_cash_flow, shares_outstanding,
            wacc, growth_rate_base, growth_rate_bull, growth_rate_bear,
            terminal_growth_rate, reasoning_ja, calculated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NOW())
        RETURNING {JP_TP_COLS}
        "#
    ))
    .bind(code)
    .bind(trigger_type)
    .bind(dcf_out.base_price)
    .bind(dcf_out.bull_price)
    .bind(dcf_out.bear_price)
    .bind(dcf_out.net_cash)
    .bind(free_cash_flow)
    .bind(shares_outstanding)
    .bind(wacc)
    .bind(growth_rate_base)
    .bind(growth_rate_bull)
    .bind(growth_rate_bear)
    .bind(terminal_growth_rate)
    .bind(reasoning_ja)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_latest_jp_theoretical_price(
    pool: &PgPool,
    code: &str,
) -> anyhow::Result<Option<JpTheoreticalPriceRow>> {
    let row = sqlx::query_as::<_, JpTheoreticalPriceRow>(&format!(
        r#"
        SELECT {JP_TP_COLS} FROM jp_theoretical_prices
        WHERE code = $1 ORDER BY calculated_at DESC, id DESC LIMIT 1
        "#
    ))
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_jp_theoretical_history(
    pool: &PgPool,
    code: &str,
) -> anyhow::Result<Vec<JpTheoreticalPriceRow>> {
    let rows = sqlx::query_as::<_, JpTheoreticalPriceRow>(&format!(
        r#"
        SELECT {JP_TP_COLS} FROM jp_theoretical_prices
        WHERE code = $1 ORDER BY calculated_at DESC, id DESC LIMIT 20
        "#
    ))
    .bind(code)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert_jp_daily_price(
    pool: &PgPool,
    bar: &JpDailyBar,
    divergence_rate: f64,
) -> anyhow::Result<JpDailyPriceRow> {
    let row = sqlx::query_as::<_, JpDailyPriceRow>(&format!(
        r#"
        INSERT INTO jp_daily_prices (
            code, trade_date, open_price, high_price, low_price, close_price,
            volume, divergence_rate, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
        ON CONFLICT (code, trade_date) DO UPDATE
        SET open_price = EXCLUDED.open_price,
            high_price = EXCLUDED.high_price,
            low_price = EXCLUDED.low_price,
            close_price = EXCLUDED.close_price,
            volume = EXCLUDED.volume,
            divergence_rate = EXCLUDED.divergence_rate,
            created_at = NOW()
        RETURNING {JP_DP_COLS}
        "#
    ))
    .bind(&bar.code)
    .bind(bar.date)
    .bind(bar.open)
    .bind(bar.high)
    .bind(bar.low)
    .bind(bar.close)
    .bind(bar.volume)
    .bind(divergence_rate)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_latest_jp_daily_price(
    pool: &PgPool,
    code: &str,
) -> anyhow::Result<Option<JpDailyPriceRow>> {
    let row = sqlx::query_as::<_, JpDailyPriceRow>(&format!(
        r#"
        SELECT {JP_DP_COLS} FROM jp_daily_prices
        WHERE code = $1 ORDER BY trade_date DESC, id DESC LIMIT 1
        "#
    ))
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_jp_daily_prices(pool: &PgPool, code: &str) -> anyhow::Result<Vec<JpDailyPriceRow>> {
    let rows = sqlx::query_as::<_, JpDailyPriceRow>(&format!(
        r#"
        SELECT {JP_DP_COLS} FROM jp_daily_prices
        WHERE code = $1 ORDER BY trade_date DESC, id DESC LIMIT 60
        "#
    ))
    .bind(code)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn insert_jp_alert_log(
    pool: &PgPool,
    code: &str,
    alert_type: &str,
    message: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO jp_alert_logs (code, alert_type, message, sent_at) VALUES ($1, $2, $3, NOW())",
    )
    .bind(code)
    .bind(alert_type)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_jp_alerts_by_code(
    pool: &PgPool,
    code: &str,
) -> anyhow::Result<Vec<JpAlertLogRow>> {
    let rows = sqlx::query_as::<_, JpAlertLogRow>(
        r#"
        SELECT id, code, alert_type, message, sent_at FROM jp_alert_logs
        WHERE code = $1 ORDER BY sent_at DESC, id DESC LIMIT 30
        "#,
    )
    .bind(code)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub fn build_jp_watchlist_item(
    w: &JpWatchlistRow,
    tp: &JpTheoreticalPriceRow,
    dp: Option<&JpDailyPriceRow>,
) -> JpWatchlistItemResponse {
    let (current_price, divergence_rate) = match dp {
        Some(d) => (d.close_price, d.divergence_rate),
        None => (0.0, 0.0),
    };
    JpWatchlistItemResponse {
        code: w.code.clone(),
        company_name: w.company_name.clone(),
        target_discount_rate: w.target_discount_rate,
        current_price,
        base_price: tp.base_price,
        bull_price: tp.bull_price,
        bear_price: tp.bear_price,
        divergence_rate,
        is_undervalued_alert: crate::dcf::is_undervalued(
            current_price,
            tp.base_price,
            w.target_discount_rate,
        ),
        wacc: tp.wacc,
        growth_rate_base: tp.growth_rate_base,
        growth_rate_bull: tp.growth_rate_bull,
        growth_rate_bear: tp.growth_rate_bear,
        terminal_growth_rate: tp.terminal_growth_rate,
        net_cash: tp.net_cash,
        free_cash_flow: tp.free_cash_flow,
        shares_outstanding: tp.shares_outstanding,
        reasoning_ja: tp.reasoning_ja.clone(),
        last_earnings_date: w.last_earnings_date,
        calculated_at: tp.calculated_at,
    }
}
