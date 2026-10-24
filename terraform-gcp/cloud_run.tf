locals {
  artifact_registry_base = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.repo.repository_id}"
  backend_image          = "${local.artifact_registry_base}/backend:${var.backend_image_tag}"
  frontend_image         = "${local.artifact_registry_base}/frontend:${var.frontend_image_tag}"
}

# ---------------------------------------------------------------------------
# Cloud Run Service: Rust Backend (Axum + SQLx + DCF Engine)
# ---------------------------------------------------------------------------
resource "google_cloud_run_v2_service" "backend" {
  project             = var.project_id
  name                = "${var.service_prefix}-backend"
  location            = var.region
  ingress             = "INGRESS_TRAFFIC_ALL"
  deletion_protection = false

  template {
    service_account = google_service_account.backend_sa.email
    timeout         = "300s"

    scaling {
      min_instance_count = 0
      max_instance_count = 2
    }

    containers {
      image = local.backend_image

      ports {
        container_port = 8080
      }

      resources {
        limits = {
          cpu    = "1000m"
          memory = "512Mi"
        }
        cpu_idle          = true
        startup_cpu_boost = true
      }

      env {
        name  = "BACKEND_PORT"
        value = "8080"
      }

      env {
        name  = "ENABLE_INTERNAL_SCHEDULER"
        value = "false"
      }

      env {
        name  = "LLM_API_BASE_URL"
        value = var.llm_api_base_url
      }

      env {
        name  = "LLM_MODEL"
        value = var.llm_model
      }

      env {
        name = "DATABASE_URL"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.app_secrets["database_url"].secret_id
            version = "latest"
          }
        }
      }

      env {
        name = "FMP_API_KEY"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.app_secrets["fmp_api_key"].secret_id
            version = "latest"
          }
        }
      }

      env {
        name = "LLM_API_KEY"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.app_secrets["llm_api_key"].secret_id
            version = "latest"
          }
        }
      }

      env {
        name = "LINE_CHANNEL_ACCESS_TOKEN"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.app_secrets["line_channel_access_token"].secret_id
            version = "latest"
          }
        }
      }

      env {
        name = "LINE_USER_ID"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.app_secrets["line_user_id"].secret_id
            version = "latest"
          }
        }
      }
    }
  }

  depends_on = [
    google_project_service.apis["run.googleapis.com"],
    google_secret_manager_secret_version.app_secret_versions,
    google_secret_manager_secret_iam_member.backend_secret_access,
  ]
}

# ---------------------------------------------------------------------------
# Cloud Run Service: Next.js Frontend Dashboard
# ---------------------------------------------------------------------------
resource "google_cloud_run_v2_service" "frontend" {
  project             = var.project_id
  name                = "${var.service_prefix}-frontend"
  location            = var.region
  ingress             = "INGRESS_TRAFFIC_ALL"
  deletion_protection = false

  template {
    service_account = google_service_account.frontend_sa.email
    timeout         = "60s"

    scaling {
      min_instance_count = 0
      max_instance_count = 2
    }

    containers {
      image = local.frontend_image

      ports {
        container_port = 3000
      }

      resources {
        limits = {
          cpu    = "1000m"
          memory = "512Mi"
        }
        cpu_idle          = true
        startup_cpu_boost = true
      }

      env {
        name  = "BACKEND_INTERNAL_URL"
        value = google_cloud_run_v2_service.backend.uri
      }

      env {
        name  = "NEXT_PUBLIC_API_URL"
        value = google_cloud_run_v2_service.backend.uri
      }
    }
  }

  depends_on = [
    google_project_service.apis["run.googleapis.com"],
    google_cloud_run_v2_service.backend,
  ]
}
