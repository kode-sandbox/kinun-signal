# ---------------------------------------------------------------------------
# 1. Cloud Monitoring Uptime Check (HTTPS Health Check for Cloud Run)
# ---------------------------------------------------------------------------
resource "google_monitoring_uptime_check_config" "frontend" {
  project      = var.project_id
  display_name = "${var.service_prefix}-frontend-uptime-check"
  period       = var.uptime_check_period
  timeout      = "10s"

  # Use minimum 3 regions to minimize request volume while preventing false positives
  selected_regions = [
    "ASIA_PACIFIC",
    "USA",
    "EUROPE",
  ]

  http_check {
    path           = var.uptime_check_path
    port           = 443
    use_ssl        = true
    validate_ssl   = true
    request_method = "GET"

    auth_info {
      username = var.basic_auth_user
      password = var.basic_auth_password
    }

    accepted_response_status_codes {
      status_class = "STATUS_CLASS_2XX"
    }
  }

  monitored_resource {
    type = "uptime_url"
    labels = {
      project_id = var.project_id
      host       = replace(google_cloud_run_v2_service.frontend.uri, "https://", "")
    }
  }

  depends_on = [
    google_project_service.apis["monitoring.googleapis.com"],
    google_cloud_run_v2_service.frontend,
  ]
}

# ---------------------------------------------------------------------------
# 2. Slack & Email Notification Channels for Uptime Alerts
# ---------------------------------------------------------------------------
resource "google_monitoring_notification_channel" "slack" {
  count = var.alert_slack_channel_name != "" && var.alert_slack_bot_token != "" ? 1 : 0

  project      = var.project_id
  display_name = "${var.service_prefix} Slack (${var.alert_slack_channel_name})"
  type         = "slack"

  labels = {
    channel_name = startswith(var.alert_slack_channel_name, "#") ? var.alert_slack_channel_name : "#${var.alert_slack_channel_name}"
  }

  sensitive_labels {
    auth_token = var.alert_slack_bot_token
  }

  depends_on = [
    google_project_service.apis["monitoring.googleapis.com"],
  ]
}

resource "google_monitoring_notification_channel" "email" {
  count = var.alert_notification_email != "" ? 1 : 0

  project      = var.project_id
  display_name = "${var.service_prefix} Alert Email"
  type         = "email"

  labels = {
    email_address = var.alert_notification_email
  }

  depends_on = [
    google_project_service.apis["monitoring.googleapis.com"],
  ]
}

locals {
  alert_notification_channels = concat(
    var.alert_notification_channel_ids,
    google_monitoring_notification_channel.slack[*].name,
    google_monitoring_notification_channel.email[*].name,
  )
}

# ---------------------------------------------------------------------------
# 3. Cloud Monitoring Alert Policy (Triggers when >= 2 regions fail for 5m)
# ---------------------------------------------------------------------------
resource "google_monitoring_alert_policy" "frontend_uptime" {
  project      = var.project_id
  display_name = "${var.service_prefix}-uptime-alert-policy"
  combiner     = "OR"
  enabled      = true

  notification_channels = local.alert_notification_channels

  conditions {
    display_name = "${var.service_prefix} Uptime Check Failure (>= 2 regions)"

    condition_threshold {
      filter          = "metric.type=\"monitoring.googleapis.com/uptime_check/check_passed\" AND resource.type=\"uptime_url\" AND metric.label.\"check_id\"=\"${google_monitoring_uptime_check_config.frontend.uptime_check_id}\""
      duration        = "300s"
      comparison      = "COMPARISON_GT"
      threshold_value = 1

      aggregations {
        alignment_period     = "300s"
        per_series_aligner   = "ALIGN_NEXT_OLDER"
        cross_series_reducer = "REDUCE_COUNT_FALSE"
        group_by_fields = [
          "resource.label.project_id",
          "resource.label.host",
        ]
      }

      trigger {
        count = 1
      }
    }
  }

  alert_strategy {
    auto_close = "1800s"
  }

  documentation {
    mime_type = "text/markdown"
    content   = <<-EOT
      ### 🚨 ${var.service_prefix} 死活監視アラート (Uptime Check Failure)

      Cloud Run サービス (`${google_cloud_run_v2_service.frontend.uri}${var.uptime_check_path}`) への死活監視（Uptime Check）が複数のリージョンから連続して失敗しました。

      #### 確認手順
      1. **Cloud Run ログの確認**: Google Cloud コンソールの [Cloud Run] または [ログ エクスプローラ] でエラーログ（5xx / 4xx）を確認してください。
      2. **エンドポイント疎通確認**: `${google_cloud_run_v2_service.frontend.uri}/healthz` および `/api/system/info` の応答状態を確認してください。
    EOT
  }

  depends_on = [
    google_project_service.apis["monitoring.googleapis.com"],
    google_monitoring_uptime_check_config.frontend,
  ]
}

# ---------------------------------------------------------------------------
# 4. Cloud Logging Exclusion Filter (Zero Logging Cost for 200 OK Health Checks)
# ---------------------------------------------------------------------------
resource "google_logging_project_exclusion" "uptime_check_200" {
  project     = var.project_id
  name        = "${var.service_prefix}-exclude-uptime-check-200"
  description = "Exclude HTTP 200 Uptime Check and /healthz access logs from Cloud Logging storage to prevent unnecessary log ingestion costs"
  filter      = "resource.type=\"cloud_run_revision\" AND httpRequest.status=200 AND (httpRequest.userAgent:\"GoogleStackdriverMonitoring-UptimeChecks\" OR httpRequest.requestUrl:\"/healthz\")"

  depends_on = [
    google_project_service.apis["logging.googleapis.com"],
  ]
}
