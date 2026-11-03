variable "project_id" {
  type        = string
  description = "Google Cloud Project ID"
}

variable "region" {
  type        = string
  default     = "asia-northeast1"
  description = "Google Cloud region (Tokyo asia-northeast1 recommended to match Supabase Tokyo)"
}

variable "service_prefix" {
  type        = string
  default     = "kinun-signal"
  description = "Prefix for Cloud Run services, Service Accounts, Secret Manager, and Cloud Scheduler jobs"
}

variable "artifact_registry_repo_id" {
  type        = string
  default     = "kinun-signal-images"
  description = "Artifact Registry Docker repository ID"
}

variable "backend_image_tag" {
  type        = string
  default     = "latest"
  description = "Docker image tag for the Rust backend service"
}

variable "frontend_image_tag" {
  type        = string
  default     = "latest"
  description = "Docker image tag for the Next.js frontend service"
}

variable "allow_public_access" {
  type        = bool
  default     = true
  description = "Whether to allow public HTTP access (allUsers) to the Frontend Cloud Run service (protected by Basic Auth). Backend is always restricted to IAM-authenticated internal callers (Frontend SA & Cloud Scheduler SA)."
}

variable "basic_auth_user" {
  type        = string
  default     = "admin"
  description = "Basic Authentication username for accessing the Frontend dashboard and BFF API"
}

variable "basic_auth_password" {
  type        = string
  sensitive   = true
  description = "Basic Authentication password for accessing the Frontend dashboard and BFF API"
}

variable "database_url" {
  type        = string
  sensitive   = true
  description = "Supabase Session Mode Pooler PostgreSQL connection URL (postgresql://postgres.[ref]:[pw]@aws-0-ap-northeast-1.pooler.supabase.com:5432/postgres)"
}

variable "fmp_api_key" {
  type        = string
  sensitive   = true
  default     = "mock_fmp_key"
  description = "Financial Modeling Prep API Key (defaults to mock_fmp_key for Graceful Fallback)"
}

variable "llm_api_base_url" {
  type        = string
  default     = "https://api.openai.com/v1"
  description = "OpenAI-compatible Chat Completions API base URL"
}

variable "llm_api_key" {
  type        = string
  sensitive   = true
  default     = "mock_llm_key"
  description = "LLM API Key (defaults to mock_llm_key for Graceful Fallback)"
}

variable "llm_model" {
  type        = string
  default     = "argon"
  description = "LLM model identifier"
}

variable "line_channel_access_token" {
  type        = string
  sensitive   = true
  default     = "mock_line_token"
  description = "LINE Messaging API Channel Access Token (defaults to mock_line_token for Graceful Fallback)"
}

variable "line_user_id" {
  type        = string
  sensitive   = true
  default     = "mock_line_user_id"
  description = "LINE Messaging API target User ID (defaults to mock_line_user_id for Graceful Fallback)"
}

variable "daily_check_cron" {
  type        = string
  default     = "30 6 * * 2-6"
  description = "Cron schedule for Daily Check & News Summary job (default: Tue-Sat 06:30 JST after US market close)"
}

variable "earnings_check_cron" {
  type        = string
  default     = "35 6 * * 2-6"
  description = "Cron schedule for Quarterly Earnings Check & DCF Recalculation job (default: Tue-Sat 06:35 JST)"
}

variable "scheduler_timezone" {
  type        = string
  default     = "Asia/Tokyo"
  description = "Time zone for Cloud Scheduler cron jobs"
}

variable "uptime_check_path" {
  type        = string
  default     = "/api/system/info"
  description = "HTTP path for Cloud Monitoring Uptime Check ('/api/system/info' checks Frontend -> Backend end-to-end without DB load; '/healthz' checks Frontend only)"
}

variable "uptime_check_period" {
  type        = string
  default     = "300s"
  description = "Frequency of Uptime Check executions ('60s', '300s', '600s', or '900s'). Default '300s' (5 min) keeps request & log volume minimal."
}

variable "alert_notification_email" {
  type        = string
  default     = ""
  description = "Optional email address to receive Cloud Monitoring Uptime Check alert notifications (leave empty to skip email channel creation)"
}

variable "alert_slack_channel_name" {
  type        = string
  default     = ""
  description = "Slack channel name (starting with '#', e.g. '#alerts') for Cloud Monitoring Uptime Check notifications"
}

variable "alert_slack_bot_token" {
  type        = string
  sensitive   = true
  default     = ""
  description = "Slack Bot User OAuth Token ('xoxb-...') with chat:write scope for Cloud Monitoring Slack notifications"
}

variable "alert_notification_channel_ids" {
  type        = list(string)
  default     = []
  description = "Optional list of existing Cloud Monitoring Notification Channel resource names (e.g. ['projects/PROJECT_ID/notificationChannels/12345'] created via GCP Console Slack OAuth)"
}
