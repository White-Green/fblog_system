#!/usr/bin/env bash
set -euo pipefail

PROJECT_NAME="$1"
HOST_NAME="$2"

PROJECT_NAME="$PROJECT_NAME" HOST_NAME="$HOST_NAME" envsubst < wrangler.preview.template.toml > wrangler.toml

echo "[fblog_system] Resources setup completed successfully!"
