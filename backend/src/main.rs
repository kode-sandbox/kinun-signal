pub mod config;
pub mod db;
pub mod dcf;
pub mod handlers;
pub mod models;
pub mod scheduler;
pub mod services;

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Router,
    routing::{delete, get, post},
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    config::AppConfig,
    handlers::AppState,
    services::{fmp::FmpClient, line::LineClient, llm::LlmClient},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stock_analyst_backend=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = AppConfig::from_env();
    info!(
        "Starting Personal Stock Analyst Backend (port={}, llm_model={}, fmp_mock={}, llm_mock={}, line_mock={})",
        cfg.backend_port,
        cfg.llm_model,
        cfg.is_fmp_mock(),
        cfg.is_llm_mock(),
        cfg.is_line_mock()
    );

    let pool = db::init_pool(&cfg.database_url).await?;
    info!("Database connected and migrations applied.");

    let fmp = FmpClient::new(cfg.clone());
    let llm = LlmClient::new(cfg.clone());
    let line = LineClient::new(cfg.clone());

    db::seed_initial_data_if_empty(&pool, &fmp, &llm, &line).await?;

    let state = Arc::new(AppState {
        pool,
        config: cfg.clone(),
        fmp,
        llm,
        line,
    });

    let _scheduler = if cfg.enable_internal_scheduler {
        Some(scheduler::start_scheduler(state.clone()).await?)
    } else {
        info!("Internal cron scheduler is disabled (ENABLE_INTERNAL_SCHEDULER=false); relying on external Cloud Scheduler.");
        None
    };

    let app = Router::new()
        .route(
            "/api/watchlist",
            get(handlers::get_watchlist).post(handlers::add_watchlist),
        )
        .route("/api/watchlist/:ticker", delete(handlers::delete_watchlist))
        .route("/api/stocks/:ticker", get(handlers::get_stock_detail))
        .route(
            "/api/stocks/:ticker/recalculate",
            post(handlers::recalculate_stock),
        )
        .route(
            "/api/jobs/daily-check",
            post(handlers::run_daily_check_handler),
        )
        .route(
            "/api/jobs/earnings-check",
            post(handlers::run_earnings_check_handler),
        )
        .route("/api/alerts", get(handlers::get_alerts))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.backend_port));
    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
