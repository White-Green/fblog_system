#!/bin/bash
set -euo pipefail

PREVIEW_NAME="$1"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

cd "$SCRIPT_DIR"

WRANGLER_OUTPUT_FILE_PATH="$SCRIPT_DIR/wrangler_deploy_output.ndjson" pnpm exec wrangler --cwd "$SCRIPT_DIR" versions upload --preview-alias "$PREVIEW_NAME"

cat "$SCRIPT_DIR/wrangler_deploy_output.ndjson" | jq -r 'select(.type == "version-upload") | .preview_alias_url' > "$SCRIPT_DIR/preview_url"
