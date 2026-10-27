# ---------------------------------------------------------------------------
# Pattern A: Cloud Scheduler -> Cloud Run Backend HTTP Endpoints (OIDC Auth)
# ---------------------------------------------------------------------------

# 1. Daily Stock Price Check & 3-Line News Summary Batch
resource "google_cloud_scheduler_job" "daily_check" {
  project          = var.project_id
  region           = var.region
  name             = "${var.service_prefix}-daily-check"
  description      = "Daily stock close price check, undervalued alert, and Japanese 3-line news summary"
  schedule         = var.daily_check_cron
  time_zone        = var.scheduler_timezone
  attempt_deadline = "300s"

  retry_config {
    retry_count = 1
  }

  http_target {
    http_method = "POST"
    uri         = "${google_cloud_run_v2_service.backend.uri}/api/jobs/daily-check"

    oidc_token {
      service_account_email = google_service_account.scheduler_sa.email
      audience              = google_cloud_run_v2_service.backend.uri
    }
  }

  depends_on = [
    google_project_service.apis["cloudscheduler.googleapis.com"],
    google_cloud_run_v2_service_iam_member.scheduler_invokes_backend,
  ]
}

# 2. Quarterly Earnings Check & DCF Theoretical Price Recalculation Batch
resource "google_cloud_scheduler_job" "earnings_check" {
  project          = var.project_id
  region           = var.region
  name             = "${var.service_prefix}-earnings-check"
  description      = "Quarterly earnings surprise check and automatic DCF theoretical price recalculation"
  schedule         = var.earnings_check_cron
  time_zone        = var.scheduler_timezone
  attempt_deadline = "300s"

  retry_config {
    retry_count = 1
  }

  http_target {
    http_method = "POST"
    uri         = "${google_cloud_run_v2_service.backend.uri}/api/jobs/earnings-check?force=false"

    oidc_token {
      service_account_email = google_service_account.scheduler_sa.email
      audience              = google_cloud_run_v2_service.backend.uri
    }
  }

  depends_on = [
    google_project_service.apis["cloudscheduler.googleapis.com"],
    google_cloud_run_v2_service_iam_member.scheduler_invokes_backend,
  ]
}
