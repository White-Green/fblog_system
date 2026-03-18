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

HOST_NAME=$(node -e 'const i=process.argv[1]; console.log(new URL(/^https?:\/\//.test(i) ? i : `https://${i}`).hostname);' "$SITE_URL")

"${SCRIPT_DIR}/../build.sh" \
  "$ARTICLE_DATA_PATH" \
  "$USER_DATA_PATH" \
  "$BUILD_DATA_PATH" \
  "$SITE_URL" \
  "$PUBLIC_KEY_PATH"

cd "$WORKING_DIR"

./crates/cloudflare_workers/snapshot_diff_for_preview.sh "$PROJECT_NAME"

mv dist crates/cloudflare_workers/public
cat events.jsonl

cd crates/cloudflare_workers
./setup_resources_for_preview.sh "$PROJECT_NAME" "$HOST_NAME"
./upload_preview.sh "$PREVIEW_NAME"
