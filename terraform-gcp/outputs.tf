output "artifact_registry_url" {
  description = "Artifact Registry base URL for pushing Docker images"
  value       = local.artifact_registry_base
}

output "docker_build_push_commands" {
  description = "Commands to build (linux/amd64 for Cloud Run on Apple Silicon Mac) and push Backend & Frontend Docker images"
  value       = <<-EOT
    # 1. Authenticate Docker with Google Cloud Artifact Registry
    gcloud auth configure-docker ${var.region}-docker.pkg.dev

    # 2. Build & Push Rust Backend Image (linux/amd64)
    docker buildx build --platform linux/amd64 \
      -t ${local.backend_image} \
      --push ../backend

    # 3. Build & Push Next.js Frontend Image (linux/amd64)
    docker buildx build --platform linux/amd64 \
      -t ${local.frontend_image} \
      --push ../frontend
  EOT
}

output "backend_url" {
  description = "Deployed Cloud Run Backend URL"
  value       = google_cloud_run_v2_service.backend.uri
}

output "frontend_url" {
  description = "Deployed Cloud Run Frontend Dashboard URL"
  value       = google_cloud_run_v2_service.frontend.uri
}

output "daily_check_scheduler_job" {
  description = "Cloud Scheduler job name for Daily Check & News Summary"
  value       = google_cloud_scheduler_job.daily_check.name
}

output "earnings_check_scheduler_job" {
  description = "Cloud Scheduler job name for Quarterly Earnings Check"
  value       = google_cloud_scheduler_job.earnings_check.name
}
