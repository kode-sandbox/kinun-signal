resource "google_artifact_registry_repository" "repo" {
  project       = var.project_id
  location      = var.region
  repository_id = var.artifact_registry_repo_id
  description   = "Docker repository for kinun-signal (Personal Stock Analyst) Backend & Frontend"
  format        = "DOCKER"

  cleanup_policies {
    id     = "keep-recent-versions"
    action = "KEEP"
    most_recent_versions {
      keep_count = 5
    }
  }

  depends_on = [
    google_project_service.apis["artifactregistry.googleapis.com"]
  ]
}
