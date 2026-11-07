use reqwest::Client;
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db;

/// LINE text messages are limited to 5,000 characters per message object.
/// Keep some headroom and split long digests at line boundaries.
const MAX_TEXT_CHARS_PER_MESSAGE: usize = 4_500;
/// The LINE push / broadcast API accepts at most 5 message objects per request.
/// The message count billed by LINE is per recipient per request, not per
/// message object, so packing a digest into one request stays one message.
const MAX_MESSAGES_PER_PUSH: usize = 5;

#[derive(Debug, Clone)]
pub struct LineClient {
    http: Client,
    config: AppConfig,
    oauth_token_cache: Arc<RwLock<Option<String>>>,
}

#[derive(Debug, Serialize)]
struct LinePushRequest<'a> {
    to: &'a str,
    messages: Vec<LineTextMessage<'a>>,
}

#[derive(Debug, Serialize)]
struct LineBroadcastRequest<'a> {
    messages: Vec<LineTextMessage<'a>>,
}

#[derive(Debug, Serialize)]
struct LineTextMessage<'a> {
    #[serde(rename = "type")]
    msg_type: &'a str,
    text: &'a str,
}

/// Split `text` into chunks of at most `max_chars` characters each,
/// preferring line boundaries. A single line longer than `max_chars`
/// is hard-split by characters. Joining chunk boundaries may drop the
/// newline that separated two chunks; the content itself is preserved.
fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    assert!(max_chars > 0);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0usize;

    for line in text.lines() {
        let line_chars = line.chars().count();
        if line_chars > max_chars {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            // Hard-split the over-long line; keep the tail as the open
            // chunk so following short lines can still be appended.
            let mut piece = String::new();
            let mut piece_chars = 0usize;
            for ch in line.chars() {
                if piece_chars == max_chars {
                    chunks.push(std::mem::take(&mut piece));
                    piece_chars = 0;
                }
                piece.push(ch);
                piece_chars += 1;
            }
            current = piece;
            current_chars = piece_chars;
        } else if current.is_empty() {
            current = line.to_string();
            current_chars = line_chars;
        } else if current_chars + 1 + line_chars <= max_chars {
            current.push('\n');
            current.push_str(line);
            current_chars += 1 + line_chars;
        } else {
            chunks.push(std::mem::take(&mut current));
            current = line.to_string();
            current_chars = line_chars;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

impl LineClient {
    pub fn new(config: AppConfig) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            config,
            oauth_token_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Resolve a valid channel access token. Uses `line_channel_access_token`
    /// if present, otherwise requests a short-lived token using OAuth
    /// client_credentials with `line_channel_id` and `line_channel_secret`.
    async fn get_access_token(&self) -> Result<String, String> {
        if !crate::config::is_mock_or_empty(&self.config.line_channel_access_token) {
            return Ok(self.config.line_channel_access_token.clone());
        }

        // Check cached token
        {
            let cache = self.oauth_token_cache.read().await;
            if let Some(token) = cache.as_ref() {
                return Ok(token.clone());
            }
        }

        if !crate::config::is_mock_or_empty(&self.config.line_channel_id)
            && !crate::config::is_mock_or_empty(&self.config.line_channel_secret)
        {
            let params = [
                ("grant_type", "client_credentials"),
                ("client_id", self.config.line_channel_id.trim()),
                ("client_secret", self.config.line_channel_secret.trim()),
            ];
            let resp = self
                .http
                .post("https://api.line.me/v2/oauth/accessToken")
                .form(&params)
                .send()
                .await
                .map_err(|e| format!("OAuth request error: {e}"))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("OAuth failed with {status}: {body}"));
            }

            #[derive(serde::Deserialize)]
            struct TokenResponse {
                access_token: String,
            }

            let token_resp: TokenResponse = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse token response: {e}"))?;

            let mut cache = self.oauth_token_cache.write().await;
            *cache = Some(token_resp.access_token.clone());
            return Ok(token_resp.access_token);
        }

        Err("No valid LINE access token or OAuth credentials configured".to_string())
    }

    /// Send already-chunked texts to LINE, batched into push/broadcast requests of
    /// at most MAX_MESSAGES_PER_PUSH message objects. If `is_line_broadcast()` is true,
    /// sends via broadcast API to all friends (convenient for single-user self-bot).
    /// Real-API failures are reported as Err with the status/body or transport error described.
    async fn push_real(&self, texts: &[String]) -> Result<(), String> {
        let token = self.get_access_token().await?;
        let is_broadcast = self.config.is_line_broadcast();

        for batch in texts.chunks(MAX_MESSAGES_PER_PUSH) {
            let messages: Vec<LineTextMessage<'_>> = batch
                .iter()
                .map(|text| LineTextMessage {
                    msg_type: "text",
                    text,
                })
                .collect();

            let resp = if is_broadcast {
                let payload = LineBroadcastRequest { messages };
                self.http
                    .post("https://api.line.me/v2/bot/message/broadcast")
                    .bearer_auth(&token)
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|err| err.to_string())?
            } else {
                let payload = LinePushRequest {
                    to: &self.config.line_user_id,
                    messages,
                };
                self.http
                    .post("https://api.line.me/v2/bot/message/push")
                    .bearer_auth(&token)
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|err| err.to_string())?
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if status == reqwest::StatusCode::UNAUTHORIZED {
                    let mut cache = self.oauth_token_cache.write().await;
                    *cache = None;
                }
                return Err(format!("{status} ({body})"));
            }
        }
        Ok(())
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
            match self.push_real(&[message.to_string()]).await {
                Ok(()) => {
                    info!(
                        "LINE notification sent successfully for {} ({})",
                        ticker, alert_type
                    );
                }
                Err(err) => {
                    warn!(
                        "LINE API push failed ({}). Falling back to console log for {}:\n{}",
                        err, ticker, message
                    );
                }
            }
        }

        db::insert_alert_log(pool, ticker, alert_type, message).await?;
        Ok(())
    }

    /// Record an alert in the history (alert_logs) without pushing it to
    /// LINE. Used by the daily check, whose per-ticker alerts are pushed
    /// together as one digest via `push_digest` while the per-ticker
    /// history (and the per-stock recent alerts) stays unchanged.
    pub async fn log_only(
        &self,
        pool: &PgPool,
        ticker: &str,
        alert_type: &str,
        message: &str,
    ) -> anyhow::Result<()> {
        db::insert_alert_log(pool, ticker, alert_type, message).await?;
        Ok(())
    }

    /// Push one combined digest message for a whole batch run. Long
    /// digests are split into multiple text objects packed into as few
    /// push requests as possible, so a daily run costs one LINE message
    /// in the common case instead of one per ticker. No alert_logs row is
    /// written here; callers record the individual alerts via `log_only`.
    pub async fn push_digest(&self, digest: &str) {
        if digest.trim().is_empty() {
            return;
        }
        if self.config.is_line_mock() {
            info!(
                "\n==================== [LINE PUSH NOTIFICATION (MOCK)] ====================\nTicker : ALL\nType   : DAILY_DIGEST\nMessage:\n{}\n=========================================================================",
                digest
            );
            return;
        }

        let texts = chunk_text(digest, MAX_TEXT_CHARS_PER_MESSAGE);
        let requests = texts.len().div_ceil(MAX_MESSAGES_PER_PUSH);
        match self.push_real(&texts).await {
            Ok(()) => {
                info!(
                    "LINE digest sent successfully ({} text(s) in {} push request(s))",
                    texts.len(),
                    requests
                );
            }
            Err(err) => {
                warn!(
                    "LINE API push failed ({}). Falling back to console log for digest:\n{}",
                    err, digest
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::chunk_text;

    #[test]
    fn short_text_stays_one_chunk() {
        let chunks = chunk_text("line1\nline2", 4_500);
        assert_eq!(chunks, vec!["line1\nline2".to_string()]);
    }

    #[test]
    fn empty_text_gives_no_chunks() {
        assert!(chunk_text("", 4_500).is_empty());
    }

    #[test]
    fn splits_at_line_boundaries() {
        let line = "a".repeat(2_000);
        let text = format!("{line}\n{line}\n{line}");
        let chunks = chunk_text(&text, 4_500);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].chars().count(), 4_001);
        assert_eq!(chunks[1], line);
        assert!(chunks.iter().all(|c| c.chars().count() <= 4_500));
    }

    #[test]
    fn hard_splits_a_single_overlong_line() {
        let text = "a".repeat(10_000);
        let chunks = chunk_text(&text, 4_500);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].chars().count(), 4_500);
        assert_eq!(chunks[1].chars().count(), 4_500);
        assert_eq!(chunks[2].chars().count(), 1_000);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn hard_split_handles_multibyte_characters() {
        let text = "あ".repeat(5_000);
        let chunks = chunk_text(&text, 4_500);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn test_broadcast_and_mock_detection() {
        let mut config = crate::config::AppConfig {
            database_url: "postgres://localhost/test".to_string(),
            backend_port: 8080,
            enable_internal_scheduler: false,
            fmp_api_key: "mock_fmp_key".to_string(),
            llm_api_base_url: "http://localhost".to_string(),
            llm_api_key: "mock_llm_key".to_string(),
            llm_model: "argon".to_string(),
            llm_news_model: "flash".to_string(),
            line_channel_id: "mock_channel_id".to_string(),
            line_channel_secret: "mock_channel_secret".to_string(),
            line_channel_access_token: "mock_line_token".to_string(),
            line_user_id: "mock_line_user_id".to_string(),
        };

        // All mock
        assert!(config.is_line_mock());
        assert!(config.is_line_broadcast());

        // With Channel ID & Secret -> not mock!
        config.line_channel_id = "test_custom_channel_id".to_string();
        config.line_channel_secret = "test_custom_channel_secret".to_string();
        assert!(!config.is_line_mock());
        assert!(config.is_line_broadcast()); // Still broadcast because user_id is mock

        // With explicit user_id -> not broadcast
        config.line_user_id = "U1234567890abcdef1234567890abcdef".to_string();
        assert!(!config.is_line_broadcast());

        // With broadcast keyword -> broadcast
        config.line_user_id = "broadcast".to_string();
        assert!(config.is_line_broadcast());
    }
}
