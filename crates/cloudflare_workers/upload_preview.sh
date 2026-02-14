#!/bin/bash
set -euo pipefail

PREVIEW_NAME="$1"

WRANGLER_OUTPUT_FILE_PATH="$(pwd)/wrangler_deploy_output.ndjson" pnpm exec wrangler versions upload --preview-alias "$PREVIEW_NAME"

cat ./wrangler_deploy_output.ndjson | jq -r 'select(.type == "version-upload") | .preview_alias_url' > ./preview_url
