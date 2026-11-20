variable "name" {
  type    = string
  default = "rust-react-dev-box"
}

variable "region" {
  type    = string
  default = env("AWS_REGION")
}

variable "ami_regions" {
  type        = list(string)
  default     = []
  description = "Further regions to copy the AMI to."
}

variable "instance_type" {
  type        = string
  default     = "c7g.2xlarge"
  description = "The Graviton instance the image is built on, not the one it runs on: any arm64 type can launch it."
}

variable "volume_size_gb" {
  type    = number
  default = 100
}

variable "encrypt_boot" {
  type    = bool
  default = true
}

variable "vpc_id" {
  type        = string
  default     = null
  description = "Build in this VPC rather than the region's default."
}

variable "subnet_id" {
  type        = string
  default     = null
  description = "A public subnet to build in, with vpc_id."
}

variable "rust_version" {
  type        = string
  default     = "1.97.1"
  description = "Keep in step with dtolnay/rust-toolchain in .github/workflows/ci.yml."
}

variable "node_version" {
  type        = string
  default     = "24"
  description = "Keep in step with mise.toml."
}

variable "pnpm_version" {
  type        = string
  default     = "12.4.2"
  description = "Keep in step with mise.toml and frontend/package.json packageManager."
}

variable "playwright_version" {
  type        = string
  default     = "1.64.1"
  description = "Keep in step with frontend/package.json: its Chromium build is pinned to the version."
}

variable "install_claude_code" {
  type    = bool
  default = true
}

variable "repo_url" {
  type        = string
  default     = ""
  description = "An app's git URL to clone into ~/app and warm (make setup, pnpm install, a debug build into sccache). Empty skips it."
}

variable "repo_ref" {
  type    = string
  default = "main"
}

variable "tags" {
  type    = map(string)
  default = {}
}
