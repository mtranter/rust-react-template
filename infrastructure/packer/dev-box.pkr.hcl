packer {
  required_plugins {
    amazon = {
      version = ">= 1.3.0"
      source  = "github.com/hashicorp/amazon"
    }
  }
}

locals {
  ami_name = "${var.name}-${formatdate("YYYYMMDD-hhmmss", timestamp())}"
  tags = merge(var.tags, {
    Name    = var.name
    Purpose = "dev-box"
    Arch    = "arm64"
  })
}

source "amazon-ebs" "dev_box" {
  region        = var.region
  instance_type = var.instance_type
  ami_name      = local.ami_name
  ami_regions   = var.ami_regions
  encrypt_boot  = var.encrypt_boot

  source_ami_filter {
    filters = {
      name                = "ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-arm64-server-*"
      architecture        = "arm64"
      root-device-type    = "ebs"
      virtualization-type = "hvm"
    }
    owners      = ["099720109477"] # Canonical
    most_recent = true
  }

  launch_block_device_mappings {
    device_name           = "/dev/sda1"
    volume_size           = var.volume_size_gb
    volume_type           = "gp3"
    delete_on_termination = true
  }

  imds_support = "v2.0"
  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 2 # containers on the box reach IMDS through docker's bridge
  }

  vpc_id                      = var.vpc_id
  subnet_id                   = var.subnet_id
  associate_public_ip_address = var.subnet_id == null ? null : true

  ssh_username = "ubuntu"

  tags            = local.tags
  run_tags        = local.tags
  snapshot_tags   = local.tags
  run_volume_tags = local.tags
}

build {
  name    = "dev-box"
  sources = ["source.amazon-ebs.dev_box"]

  provisioner "shell" {
    inline = ["cloud-init status --wait || true"]
  }

  provisioner "shell" {
    script          = "${path.root}/scripts/01-system.sh"
    execute_command = "chmod +x {{ .Path }}; sudo -E bash {{ .Path }}"
  }

  provisioner "shell" {
    script = "${path.root}/scripts/02-toolchains.sh"
    environment_vars = [
      "RUST_VERSION=${var.rust_version}",
      "NODE_VERSION=${var.node_version}",
      "PNPM_VERSION=${var.pnpm_version}",
      "PLAYWRIGHT_VERSION=${var.playwright_version}",
      "INSTALL_CLAUDE_CODE=${var.install_claude_code}",
    ]
  }

  provisioner "file" {
    source      = "${path.root}/../../docker-compose.yml"
    destination = "/tmp/docker-compose.yml"
  }

  provisioner "shell" {
    script = "${path.root}/scripts/03-images.sh"
  }

  provisioner "shell" {
    script = "${path.root}/scripts/04-repo.sh"
    environment_vars = [
      "REPO_URL=${var.repo_url}",
      "REPO_REF=${var.repo_ref}",
    ]
  }

  provisioner "shell" {
    script          = "${path.root}/scripts/99-cleanup.sh"
    execute_command = "chmod +x {{ .Path }}; sudo -E bash {{ .Path }}"
  }

  post-processor "manifest" {
    output     = "${path.root}/manifest.json"
    strip_path = true
  }
}
