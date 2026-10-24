output "instance_id" {
  value       = aws_instance.spot_verify.id
  description = "The ID of the EC2 instance"
}

output "ssm_start_session_command" {
  value       = "aws ssm start-session --target ${aws_instance.spot_verify.id}"
  description = "Command to connect to the instance via SSM"
}
