# 1. Backend Cloud Run Runtime Service Account
resource "google_service_account" "backend_sa" {
  project      = var.project_id
  account_id   = "${var.service_prefix}-backend-sa"
  display_name = "Kinun Signal Backend Cloud Run Service Account"

  depends_on = [
    google_project_service.apis["iam.googleapis.com"]
  ]
}

# Grant Backend SA access to read application secrets from Secret Manager
resource "google_secret_manager_secret_iam_member" "backend_secret_access" {
  for_each = google_secret_manager_secret.app_secrets

  project   = var.project_id
  secret_id = each.value.secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.backend_sa.email}"
}

# 2. Frontend Cloud Run Runtime Service Account
resource "google_service_account" "frontend_sa" {
  project      = var.project_id
  account_id   = "${var.service_prefix}-frontend-sa"
  display_name = "Kinun Signal Frontend Cloud Run Service Account"

  depends_on = [
    google_project_service.apis["iam.googleapis.com"]
  ]
}

# Allow Frontend SA to read Basic Auth secrets from Secret Manager
resource "google_secret_manager_secret_iam_member" "frontend_secret_access" {
  for_each = toset(["basic_auth_user", "basic_auth_password"])

  project   = var.project_id
  secret_id = google_secret_manager_secret.app_secrets[each.key].secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.frontend_sa.email}"
}

# Allow Frontend SA to invoke Backend Cloud Run service
resource "google_cloud_run_v2_service_iam_member" "frontend_invokes_backend" {
  project  = var.project_id
  location = var.region
  name     = google_cloud_run_v2_service.backend.name
  role     = "roles/run.invoker"
  member   = "serviceAccount:${google_service_account.frontend_sa.email}"
}

# 3. Cloud Scheduler Service Account (Pattern A: HTTP + OIDC Trigger)
resource "google_service_account" "scheduler_sa" {
  project      = var.project_id
  account_id   = "${var.service_prefix}-scheduler-sa"
  display_name = "Kinun Signal Cloud Scheduler Service Account"

  depends_on = [
    google_project_service.apis["iam.googleapis.com"]
  ]
}

# Allow Cloud Scheduler SA to invoke Backend Cloud Run batch endpoints
resource "google_cloud_run_v2_service_iam_member" "scheduler_invokes_backend" {
  project  = var.project_id
  location = var.region
  name     = google_cloud_run_v2_service.backend.name
  role     = "roles/run.invoker"
  member   = "serviceAccount:${google_service_account.scheduler_sa.email}"
}

# 4. Public HTTP Access for Frontend only (Backend is private and only invokable by Frontend SA & Scheduler SA)
resource "google_cloud_run_v2_service_iam_member" "frontend_public" {
  count = var.allow_public_access ? 1 : 0

  project  = var.project_id
  location = var.region
  name     = google_cloud_run_v2_service.frontend.name
  role     = "roles/run.invoker"
  member   = "allUsers"
}
