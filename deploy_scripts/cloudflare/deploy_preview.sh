#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKING_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PREVIEW_NAME="$1"
ARTICLE_DATA_PATH="$2"
USER_DATA_PATH="$3"
BUILD_DATA_PATH="$4"
PROJECT_NAME="$5"
SITE_URL="$6"
PUBLIC_KEY_PATH="$7"

: "${CLOUDFLARE_ACCOUNT_ID:?CLOUDFLARE_ACCOUNT_ID is required}"
: "${CLOUDFLARE_API_TOKEN:?CLOUDFLARE_API_TOKEN is required}"

CF_ACCOUNT_ID_VALUE="$CLOUDFLARE_ACCOUNT_ID"
CF_API_TOKEN_VALUE="$CLOUDFLARE_API_TOKEN"
unset CLOUDFLARE_ACCOUNT_ID CLOUDFLARE_API_TOKEN CF_ACCOUNT_ID CF_API_TOKEN

HOST_NAME=$(node -e 'const i=process.argv[1]; console.log(new URL(/^https?:\/\//.test(i) ? i : `https://${i}`).hostname);' "$SITE_URL")

"${SCRIPT_DIR}/../build.sh" \
  "$ARTICLE_DATA_PATH" \
  "$USER_DATA_PATH" \
  "$BUILD_DATA_PATH" \
  "$SITE_URL" \
  "$PUBLIC_KEY_PATH" \
  "false"

cd "$WORKING_DIR"

CLOUDFLARE_ACCOUNT_ID="$CF_ACCOUNT_ID_VALUE" CLOUDFLARE_API_TOKEN="$CF_API_TOKEN_VALUE" ./crates/cloudflare_workers/snapshot_diff_for_preview.sh "$PROJECT_NAME"

mv dist crates/cloudflare_workers/public
echo "=== events ==="
cat events.jsonl
echo "=== events ==="

cd crates/cloudflare_workers
./setup_resources_for_preview.sh "$PROJECT_NAME" "$HOST_NAME"
CLOUDFLARE_ACCOUNT_ID="$CF_ACCOUNT_ID_VALUE" CLOUDFLARE_API_TOKEN="$CF_API_TOKEN_VALUE" ./upload_preview.sh "$PREVIEW_NAME"
