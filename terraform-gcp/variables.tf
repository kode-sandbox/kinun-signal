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
  description = "Whether to allow unauthenticated public HTTP access (allUsers) to Frontend and Backend Cloud Run services"
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
