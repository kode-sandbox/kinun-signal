use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub backend_port: u16,
    pub enable_internal_scheduler: bool,
    pub fmp_api_key: String,
    pub llm_api_base_url: String,
    pub llm_api_key: String,
    pub llm_model: String,
    pub line_channel_access_token: String,
    pub line_user_id: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();
        let _ = dotenvy::from_filename("../.env");

        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/stock_analyst".to_string());
        let backend_port = env::var("BACKEND_PORT")
            .or_else(|_| env::var("PORT"))
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);

        let enable_internal_scheduler = env::var("ENABLE_INTERNAL_SCHEDULER")
            .map(|v| {
                let lower = v.trim().to_ascii_lowercase();
                lower != "false" && lower != "0" && lower != "no"
            })
            .unwrap_or(true);

        let fmp_api_key = env::var("FMP_API_KEY").unwrap_or_else(|_| "mock_fmp_key".to_string());

        let raw_llm_base = env::var("LLM_API_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let llm_api_base_url = sanitize_url(&raw_llm_base);

        let llm_api_key = env::var("LLM_API_KEY").unwrap_or_else(|_| "mock_llm_key".to_string());
        let llm_model = env::var("LLM_MODEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "argon".to_string());

        let line_channel_access_token = env::var("LINE_CHANNEL_ACCESS_TOKEN")
            .unwrap_or_else(|_| "mock_line_token".to_string());
        let line_user_id = env::var("LINE_USER_ID")
            .unwrap_or_else(|_| "mock_line_user_id".to_string());

        Self {
            database_url,
            backend_port,
            enable_internal_scheduler,
            fmp_api_key,
            llm_api_base_url,
            llm_api_key,
            llm_model,
            line_channel_access_token,
            line_user_id,
        }
    }

    pub fn is_fmp_mock(&self) -> bool {
        is_mock_or_empty(&self.fmp_api_key)
    }

    pub fn is_llm_mock(&self) -> bool {
        is_mock_or_empty(&self.llm_api_key)
    }

    pub fn is_line_mock(&self) -> bool {
        is_mock_or_empty(&self.line_channel_access_token) || is_mock_or_empty(&self.line_user_id)
    }
}

fn is_mock_or_empty(val: &str) -> bool {
    let trimmed = val.trim();
    trimmed.is_empty()
        || trimmed.starts_with("mock_")
        || trimmed.starts_with("your_")
        || trimmed == "changeme"
}

fn sanitize_url(raw: &str) -> String {
    let trimmed = raw.trim();
    // Handle markdown format like [https://api.openai.com/v1](https://api.openai.com/v1) if present
    if let Some(start) = trimmed.find("](") {
        if let Some(end) = trimmed[start + 2..].find(')') {
            return trimmed[start + 2..start + 2 + end].trim_end_matches('/').to_string();
        }
    }
    trimmed.trim_end_matches('/').to_string()
}
