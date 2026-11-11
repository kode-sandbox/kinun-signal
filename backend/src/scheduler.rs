use std::sync::Arc;

use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::handlers::{self, AppState};
use crate::jp_handlers;

pub async fn start_scheduler(state: Arc<AppState>) -> anyhow::Result<JobScheduler> {
    let sched = JobScheduler::new().await?;

    let state_clone = state.clone();
    let job = Job::new_async("0 30 21 * * Tue-Sat", move |_uuid, _l| {
        let st = state_clone.clone();
        Box::pin(async move {
            info!("Starting scheduled daily-check and earnings-check batch jobs...");
            if let Err(err) = handlers::execute_daily_check(&st).await {
                error!("Scheduled daily-check failed: {}", err);
            }
            if let Err(err) = handlers::execute_earnings_check(&st, false).await {
                error!("Scheduled earnings-check failed: {}", err);
            }
            info!("Completed scheduled daily-check and earnings-check batch jobs.");
        })
    })?;

    sched.add(job).await?;

    // JP daily-check: weekdays 17:00 JST = 08:00 UTC ('0 0 8 * * Mon-Fri'),
    // after the TSE close (15:30 JST) of the same day.
    let jp_state = state.clone();
    let jp_job = Job::new_async("0 0 8 * * Mon-Fri", move |_uuid, _l| {
        let st = jp_state.clone();
        Box::pin(async move {
            info!("Starting scheduled JP daily-check batch job...");
            match jp_handlers::execute_jp_daily_check(&st).await {
                Ok(summary) => {
                    info!(
                        "JP daily-check completed: processed={}, alerts={}",
                        summary.processed_count, summary.alerts_triggered
                    );
                }
                Err(err) => error!("Scheduled JP daily-check failed: {}", err),
            }
        })
    })?;
    sched.add(jp_job).await?;

    sched.start().await?;
    info!("Cron scheduler started with schedule: '0 30 21 * * Tue-Sat' (UTC) + JP '0 0 8 * * Mon-Fri' (UTC)");

    Ok(sched)
}
