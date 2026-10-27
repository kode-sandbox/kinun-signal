locals {
  secret_values = {
    database_url              = var.database_url
    fmp_api_key               = var.fmp_api_key
    llm_api_key               = var.llm_api_key
    line_channel_access_token = var.line_channel_access_token
    line_user_id              = var.line_user_id
    basic_auth_user           = var.basic_auth_user
    basic_auth_password       = var.basic_auth_password
  }
}

resource "google_secret_manager_secret" "app_secrets" {
  for_each = nonsensitive(toset(keys(local.secret_values)))

  project   = var.project_id
  secret_id = "${var.service_prefix}-${replace(each.key, "_", "-")}"

  replication {
    auto {}
  }

  depends_on = [
    google_project_service.apis["secretmanager.googleapis.com"]
  ]
}

resource "google_secret_manager_secret_version" "app_secret_versions" {
  for_each = nonsensitive(toset(keys(local.secret_values)))

  secret      = google_secret_manager_secret.app_secrets[each.key].id
  secret_data = local.secret_values[each.key]
}
