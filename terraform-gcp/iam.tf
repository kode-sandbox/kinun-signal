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

# ---------------------------------------------------------------------------
# 5. GitHub Actions Keyless Authentication (Workload Identity Federation / OIDC)
# ---------------------------------------------------------------------------
resource "google_service_account" "github_deployer_sa" {
  project      = var.project_id
  account_id   = "${var.service_prefix}-github-deployer"
  display_name = "Kinun Signal GitHub Actions Deployer Service Account"

  depends_on = [
    google_project_service.apis["iam.googleapis.com"]
  ]
}

# Allow GitHub Deployer SA to push Docker images to Artifact Registry
resource "google_project_iam_member" "github_deployer_ar_writer" {
  project = var.project_id
  role    = "roles/artifactregistry.writer"
  member  = "serviceAccount:${google_service_account.github_deployer_sa.email}"
}

# Allow GitHub Deployer SA to deploy Cloud Run services
resource "google_project_iam_member" "github_deployer_run_admin" {
  project = var.project_id
  role    = "roles/run.admin"
  member  = "serviceAccount:${google_service_account.github_deployer_sa.email}"
}

# Allow GitHub Deployer SA to act as Backend & Frontend Runtime Service Accounts during Cloud Run deployment
resource "google_service_account_iam_member" "github_deployer_act_as_backend_sa" {
  service_account_id = google_service_account.backend_sa.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.github_deployer_sa.email}"
}

resource "google_service_account_iam_member" "github_deployer_act_as_frontend_sa" {
  service_account_id = google_service_account.frontend_sa.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.github_deployer_sa.email}"
}

resource "google_iam_workload_identity_pool" "github_pool" {
  project                   = var.project_id
  workload_identity_pool_id = "${var.service_prefix}-github-pool"
  display_name              = "GitHub Actions Pool"
  description               = "Workload Identity Pool for GitHub Actions keyless CI/CD"

  depends_on = [
    google_project_service.apis["iam.googleapis.com"],
    google_project_service.apis["iamcredentials.googleapis.com"],
    google_project_service.apis["sts.googleapis.com"],
  ]
}

resource "google_iam_workload_identity_pool_provider" "github_provider" {
  project                            = var.project_id
  workload_identity_pool_id          = google_iam_workload_identity_pool.github_pool.workload_identity_pool_id
  workload_identity_pool_provider_id = "github-provider"
  display_name                       = "GitHub Actions OIDC Provider"

  attribute_mapping = {
    "google.subject"       = "assertion.sub"
    "attribute.actor"      = "assertion.actor"
    "attribute.repository" = "assertion.repository"
  }

  attribute_condition = join(" || ", [
    for repo in var.github_repositories : "assertion.repository == '${repo}'"
  ])

  oidc {
    issuer_uri = "https://token.actions.githubusercontent.com"
  }
}

resource "google_service_account_iam_member" "github_oidc_binding" {
  for_each = toset(var.github_repositories)

  service_account_id = google_service_account.github_deployer_sa.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "principalSet://iam.googleapis.com/${google_iam_workload_identity_pool.github_pool.name}/attribute.repository/${each.value}"
}
