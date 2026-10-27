use chrono::{NaiveDate, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing::info;

use crate::dcf::{self, DcfInput, DcfOutput};
use crate::models::{
    AlertLogRow, DailyPriceRow, TheoreticalPriceRow, WatchlistItemResponse, WatchlistRow,
};
use crate::services::{fmp::FmpClient, line::LineClient, llm::{LlmClient, LlmDcfParams}};

pub async fn init_pool(database_url: &str) -> anyhow::Result<PgPool> {
    // Attempt to connect to the PostgreSQL database with retries, handling the case where the container
    // may not be ready yet when the backend starts. This prevents a startup failure with "role \"postgres\" does not exist".
    let mut attempts = 0usize;
    let max_attempts = 20usize;
    let mut pool: Option<PgPool> = None;
    while attempts < max_attempts {
        attempts += 1;
        match PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await {
            Ok(p) => {
                pool = Some(p);
                break;
            }
            Err(e) => {
                tracing::warn!("Database connection attempt {}/{} failed: {}", attempts, max_attempts, e);
                // Exponential backoff: increase wait time up to 5 seconds.
                let wait_secs = std::cmp::min(2 * attempts as u64, 5);
                tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
            }
        }
    }
    let pool = pool.ok_or_else(|| anyhow::anyhow!("Failed to connect to PostgreSQL after {} attempts", max_attempts))?;

    // Run migrations after establishing the connection.
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn upsert_watchlist(
    pool: &PgPool,
    ticker: &str,
    company_name: &str,
    target_discount_rate: f64,
    last_earnings_date: Option<NaiveDate>,
) -> anyhow::Result<WatchlistRow> {
    let row = sqlx::query_as::<_, WatchlistRow>(
        r#"
        INSERT INTO watchlists (ticker, company_name, target_discount_rate, last_earnings_date, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW(), NOW())
        ON CONFLICT (ticker) DO UPDATE
        SET company_name = EXCLUDED.company_name,
            target_discount_rate = EXCLUDED.target_discount_rate,
            last_earnings_date = COALESCE(EXCLUDED.last_earnings_date, watchlists.last_earnings_date),
            updated_at = NOW()
        RETURNING ticker, company_name, target_discount_rate, last_earnings_date, created_at, updated_at
        "#,
    )
    .bind(ticker)
    .bind(company_name)
    .bind(target_discount_rate)
    .bind(last_earnings_date)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn update_watchlist_earnings_date(
    pool: &PgPool,
    ticker: &str,
    last_earnings_date: NaiveDate,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE watchlists
        SET last_earnings_date = $2, updated_at = NOW()
        WHERE ticker = $1
        "#,
    )
    .bind(ticker)
    .bind(last_earnings_date)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_watchlist(pool: &PgPool, ticker: &str) -> anyhow::Result<bool> {
    let res = sqlx::query("DELETE FROM watchlists WHERE ticker = $1")
        .bind(ticker)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn get_all_watchlists(pool: &PgPool) -> anyhow::Result<Vec<WatchlistRow>> {
    let rows = sqlx::query_as::<_, WatchlistRow>(
        "SELECT ticker, company_name, target_discount_rate, last_earnings_date, created_at, updated_at FROM watchlists ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_watchlist_by_ticker(
    pool: &PgPool,
    ticker: &str,
) -> anyhow::Result<Option<WatchlistRow>> {
    let row = sqlx::query_as::<_, WatchlistRow>(
        "SELECT ticker, company_name, target_discount_rate, last_earnings_date, created_at, updated_at FROM watchlists WHERE ticker = $1",
    )
    .bind(ticker)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_theoretical_price(
    pool: &PgPool,
    ticker: &str,
    trigger_type: &str,
    dcf_out: &DcfOutput,
    free_cash_flow: f64,
    shares_outstanding: f64,
    params: &LlmDcfParams,
) -> anyhow::Result<TheoreticalPriceRow> {
    let row = sqlx::query_as::<_, TheoreticalPriceRow>(
        r#"
        INSERT INTO theoretical_prices (
            ticker, trigger_type, base_price, bull_price, bear_price,
            net_cash, free_cash_flow, shares_outstanding,
            wacc, growth_rate_base, growth_rate_bull, growth_rate_bear,
            terminal_growth_rate, reasoning_ja, calculated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NOW())
        RETURNING id, ticker, trigger_type, base_price, bull_price, bear_price,
                  net_cash, free_cash_flow, shares_outstanding,
                  wacc, growth_rate_base, growth_rate_bull, growth_rate_bear,
                  terminal_growth_rate, reasoning_ja, calculated_at
        "#,
    )
    .bind(ticker)
    .bind(trigger_type)
    .bind(dcf_out.base_price)
    .bind(dcf_out.bull_price)
    .bind(dcf_out.bear_price)
    .bind(dcf_out.net_cash)
    .bind(free_cash_flow)
    .bind(shares_outstanding)
    .bind(params.wacc)
    .bind(params.growth_rate_base)
    .bind(params.growth_rate_bull)
    .bind(params.growth_rate_bear)
    .bind(params.terminal_growth_rate)
    .bind(&params.reasoning_ja)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn get_latest_theoretical_price(
    pool: &PgPool,
    ticker: &str,
) -> anyhow::Result<Option<TheoreticalPriceRow>> {
    let row = sqlx::query_as::<_, TheoreticalPriceRow>(
        r#"
        SELECT id, ticker, trigger_type, base_price, bull_price, bear_price,
               net_cash, free_cash_flow, shares_outstanding,
               wacc, growth_rate_base, growth_rate_bull, growth_rate_bear,
               terminal_growth_rate, reasoning_ja, calculated_at
        FROM theoretical_prices
        WHERE ticker = $1
        ORDER BY calculated_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(ticker)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_theoretical_price_history(
    pool: &PgPool,
    ticker: &str,
) -> anyhow::Result<Vec<TheoreticalPriceRow>> {
    let rows = sqlx::query_as::<_, TheoreticalPriceRow>(
        r#"
        SELECT id, ticker, trigger_type, base_price, bull_price, bear_price,
               net_cash, free_cash_flow, shares_outstanding,
               wacc, growth_rate_base, growth_rate_bull, growth_rate_bear,
               terminal_growth_rate, reasoning_ja, calculated_at
        FROM theoretical_prices
        WHERE ticker = $1
        ORDER BY calculated_at DESC, id DESC
        LIMIT 20
        "#,
    )
    .bind(ticker)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert_daily_price(
    pool: &PgPool,
    ticker: &str,
    trade_date: NaiveDate,
    close_price: f64,
    divergence_rate: f64,
) -> anyhow::Result<DailyPriceRow> {
    let row = sqlx::query_as::<_, DailyPriceRow>(
        r#"
        INSERT INTO daily_prices (ticker, trade_date, close_price, divergence_rate, created_at)
        VALUES ($1, $2, $3, $4, NOW())
        ON CONFLICT (ticker, trade_date) DO UPDATE
        SET close_price = EXCLUDED.close_price,
            divergence_rate = EXCLUDED.divergence_rate,
            created_at = NOW()
        RETURNING id, ticker, trade_date, close_price, divergence_rate, created_at
        "#,
    )
    .bind(ticker)
    .bind(trade_date)
    .bind(close_price)
    .bind(divergence_rate)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn get_latest_daily_price(
    pool: &PgPool,
    ticker: &str,
) -> anyhow::Result<Option<DailyPriceRow>> {
    let row = sqlx::query_as::<_, DailyPriceRow>(
        r#"
        SELECT id, ticker, trade_date, close_price, divergence_rate, created_at
        FROM daily_prices
        WHERE ticker = $1
        ORDER BY trade_date DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(ticker)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_daily_prices(pool: &PgPool, ticker: &str) -> anyhow::Result<Vec<DailyPriceRow>> {
    let rows = sqlx::query_as::<_, DailyPriceRow>(
        r#"
        SELECT id, ticker, trade_date, close_price, divergence_rate, created_at
        FROM daily_prices
        WHERE ticker = $1
        ORDER BY trade_date DESC, id DESC
        LIMIT 30
        "#,
    )
    .bind(ticker)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn insert_alert_log(
    pool: &PgPool,
    ticker: &str,
    alert_type: &str,
    message: &str,
) -> anyhow::Result<AlertLogRow> {
    let row = sqlx::query_as::<_, AlertLogRow>(
        r#"
        INSERT INTO alert_logs (ticker, alert_type, message, sent_at)
        VALUES ($1, $2, $3, NOW())
        RETURNING id, ticker, alert_type, message, sent_at
        "#,
    )
    .bind(ticker)
    .bind(alert_type)
    .bind(message)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn get_recent_alerts(pool: &PgPool, limit: i64) -> anyhow::Result<Vec<AlertLogRow>> {
    let rows = sqlx::query_as::<_, AlertLogRow>(
        r#"
        SELECT id, ticker, alert_type, message, sent_at
        FROM alert_logs
        ORDER BY sent_at DESC, id DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_alerts_by_ticker(
    pool: &PgPool,
    ticker: &str,
) -> anyhow::Result<Vec<AlertLogRow>> {
    let rows = sqlx::query_as::<_, AlertLogRow>(
        r#"
        SELECT id, ticker, alert_type, message, sent_at
        FROM alert_logs
        WHERE ticker = $1
        ORDER BY sent_at DESC, id DESC
        LIMIT 20
        "#,
    )
    .bind(ticker)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn build_watchlist_item_response(
    pool: &PgPool,
    w: &WatchlistRow,
) -> anyhow::Result<Option<WatchlistItemResponse>> {
    let Some(tp) = get_latest_theoretical_price(pool, &w.ticker).await? else {
        return Ok(None);
    };
    let dp = get_latest_daily_price(pool, &w.ticker).await?;
    let current_price = dp.as_ref().map(|d| d.close_price).unwrap_or(tp.base_price);
    let divergence_rate = dcf::calculate_divergence_rate(current_price, tp.base_price);
    let is_undervalued_alert =
        dcf::is_undervalued(current_price, tp.base_price, w.target_discount_rate);

    Ok(Some(WatchlistItemResponse {
        ticker: w.ticker.clone(),
        company_name: w.company_name.clone(),
        target_discount_rate: w.target_discount_rate,
        current_price,
        base_price: tp.base_price,
        bull_price: tp.bull_price,
        bear_price: tp.bear_price,
        divergence_rate,
        is_undervalued_alert,
        wacc: tp.wacc,
        growth_rate_base: tp.growth_rate_base,
        growth_rate_bull: tp.growth_rate_bull,
        growth_rate_bear: tp.growth_rate_bear,
        terminal_growth_rate: tp.terminal_growth_rate,
        net_cash: tp.net_cash,
        free_cash_flow: tp.free_cash_flow,
        shares_outstanding: tp.shares_outstanding,
        reasoning_ja: tp.reasoning_ja,
        last_earnings_date: w.last_earnings_date,
        calculated_at: tp.calculated_at,
    }))
}

pub async fn seed_initial_data_if_empty(
    pool: &PgPool,
    fmp: &FmpClient,
    llm: &LlmClient,
    line: &LineClient,
) -> anyhow::Result<()> {
    let existing = get_all_watchlists(pool).await?;
    if !existing.is_empty() {
        info!(
            "Database already has {} watchlist stocks. Skipping initial seed.",
            existing.len()
        );
        return Ok(());
    }

    info!("Watchlist table is empty. Seeding initial stocks: AAPL and NVDA...");

    let seed_targets = vec![("AAPL", 15.0), ("NVDA", 20.0)];

    for (ticker, target_discount_rate) in seed_targets {
        let fin = fmp.fetch_financials(ticker).await;
        let news = fmp.fetch_news(ticker).await;

        upsert_watchlist(
            pool,
            &fin.ticker,
            &fin.company_name,
            target_discount_rate,
            fin.latest_earnings_date,
        )
        .await?;

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

        insert_theoretical_price(
            pool,
            &fin.ticker,
            "INITIAL",
            &dcf_out,
            fin.free_cash_flow,
            fin.shares_outstanding,
            &params,
        )
        .await?;

        let today = Utc::now().date_naive();
        let div_rate = dcf::calculate_divergence_rate(fin.current_price, dcf_out.base_price);
        upsert_daily_price(pool, &fin.ticker, today, fin.current_price, div_rate).await?;

        let init_msg = format!(
            "📊【新規銘柄登録＆理論株価算出】\n銘柄: {} ({})\n現在株価: ${:.2}\n理論株価(Base): ${:.2} (乖離率: {:+.2}%)\n強気(Bull): ${:.2} / 弱気(Bear): ${:.2}\nWACC: {:.1}% / 基本成長率: {:.1}%\n\n📝 推論サマリー:\n{}",
            fin.company_name,
            fin.ticker,
            fin.current_price,
            dcf_out.base_price,
            div_rate,
            dcf_out.bull_price,
            dcf_out.bear_price,
            params.wacc * 100.0,
            params.growth_rate_base * 100.0,
            params.reasoning_ja
        );
        line.send_and_log(pool, &fin.ticker, "INITIAL_CALC", &init_msg)
            .await?;

        if dcf::is_undervalued(fin.current_price, dcf_out.base_price, target_discount_rate) {
            let alert_msg = format!(
                "🚨【割安アラート】{} が理論株価(${:.2})から{:.2}%割安の ${:.2} に到達しました！（目標割安率: {:.0}%）",
                fin.ticker,
                dcf_out.base_price,
                div_rate.abs(),
                fin.current_price,
                target_discount_rate
            );
            line.send_and_log(pool, &fin.ticker, "UNDERVALUED", &alert_msg)
                .await?;
        }
    }

    info!("Initial seed for AAPL and NVDA completed.");
    Ok(())
}
