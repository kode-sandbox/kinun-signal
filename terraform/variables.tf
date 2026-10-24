variable "project_name" {
  type        = string
  default     = "spot-sandbox"
  description = "The name of the project"
}

variable "aws_region" {
  type        = string
  default     = "ap-northeast-1"
  description = "The AWS region to deploy resources to"
}

variable "instance_type" {
  type        = string
  default     = "t4g.medium"
  description = "The EC2 instance type (make sure it matches the architecture of the AMI)"
}

variable "spot_instance_interruption_behavior" {
  type        = string
  default     = "terminate"
  description = "The behavior when a spot instance is interrupted (terminate, stop, or hibernate)"
}

variable "root_volume_size" {
  type        = number
  default     = 20
  description = "The size of the root volume in GB"
}

variable "monthly_budget_amount" {
  type        = string
  default     = "10.0"
  description = "The monthly budget amount in USD"
}

variable "budget_notification_email" {
  type        = string
  default     = ""
  description = "The email address to send budget notifications to. Leave empty to disable budget creation."
}
