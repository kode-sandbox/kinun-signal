use reqwest::Client;
use serde::Serialize;
use sqlx::PgPool;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db;

#[derive(Debug, Clone)]
pub struct LineClient {
    http: Client,
    config: AppConfig,
}

#[derive(Debug, Serialize)]
struct LinePushRequest<'a> {
    to: &'a str,
    messages: Vec<LineTextMessage<'a>>,
}

#[derive(Debug, Serialize)]
struct LineTextMessage<'a> {
    #[serde(rename = "type")]
    msg_type: &'a str,
    text: &'a str,
}

impl LineClient {
    pub fn new(config: AppConfig) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            config,
        }
    }

    pub async fn send_and_log(
        &self,
        pool: &PgPool,
        ticker: &str,
        alert_type: &str,
        message: &str,
    ) -> anyhow::Result<()> {
        if self.config.is_line_mock() {
            info!(
                "\n==================== [LINE PUSH NOTIFICATION (MOCK)] ====================\nTicker : {}\nType   : {}\nMessage:\n{}\n=========================================================================",
                ticker, alert_type, message
            );
        } else {
            let payload = LinePushRequest {
                to: &self.config.line_user_id,
                messages: vec![LineTextMessage {
                    msg_type: "text",
                    text: message,
                }],
            };

            match self
                .http
                .post("https://api.line.me/v2/bot/message/push")
                .bearer_auth(&self.config.line_channel_access_token)
                .json(&payload)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    info!(
                        "LINE notification sent successfully for {} ({})",
                        ticker, alert_type
                    );
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!(
                        "LINE API returned {} ({}). Falling back to console log for {}:\n{}",
                        status, body, ticker, message
                    );
                }
                Err(err) => {
                    warn!(
                        "LINE API request failed ({}). Falling back to console log for {}:\n{}",
                        err, ticker, message
                    );
                }
            }
        }

        db::insert_alert_log(pool, ticker, alert_type, message).await?;
        Ok(())
    }
}
