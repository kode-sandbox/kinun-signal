locals {
  # Determine architecture based on instance type
  architecture = can(regex("^.*g.*\\..*", var.instance_type)) ? "arm64" : "x86_64"
}

data "aws_ami" "amazon_linux_2023" {
  most_recent = true
  owners      = ["amazon"]

  filter {
    name   = "name"
    values = ["al2023-ami-2023.*-${local.architecture}"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }
}

resource "aws_instance" "spot_verify" {
  ami           = data.aws_ami.amazon_linux_2023.id
  instance_type = var.instance_type
  subnet_id     = aws_subnet.public.id

  vpc_security_group_ids = [aws_security_group.main.id]
  iam_instance_profile   = aws_iam_instance_profile.ssm_profile.name

  instance_market_options {
    market_type = "spot"
    spot_options {
      instance_interruption_behavior = var.spot_instance_interruption_behavior
    }
  }

  root_block_device {
    volume_type = "gp3"
    volume_size = var.root_volume_size
    encrypted   = true
  }

  user_data = <<-EOF
    #!/bin/bash
    dnf update -y
    dnf install -y docker
    systemctl enable docker
    systemctl start docker
    usermod -aG docker ssm-user
  EOF

  tags = {
    Name = "${var.project_name}-instance"
  }
}
