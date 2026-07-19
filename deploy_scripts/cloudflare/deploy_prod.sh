#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKING_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ARTICLE_DATA_PATH="$1"
USER_DATA_PATH="$2"
BUILD_DATA_PATH="$3"
PROJECT_NAME="$4"
SITE_URL="$5"
PUBLIC_KEY_PATH="$6"

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
  "true"

cd "$WORKING_DIR"

CLOUDFLARE_ACCOUNT_ID="$CF_ACCOUNT_ID_VALUE" CLOUDFLARE_API_TOKEN="$CF_API_TOKEN_VALUE" ./crates/cloudflare_workers/update_snapshot.sh "$PROJECT_NAME"

mv dist crates/cloudflare_workers/public
echo "=== events ==="
cat events.jsonl
echo "=== events ==="

cd crates/cloudflare_workers
CLOUDFLARE_ACCOUNT_ID="$CF_ACCOUNT_ID_VALUE" CLOUDFLARE_API_TOKEN="$CF_API_TOKEN_VALUE" ./setup_resources.sh "$PROJECT_NAME" "$HOST_NAME"
CLOUDFLARE_ACCOUNT_ID="$CF_ACCOUNT_ID_VALUE" CLOUDFLARE_API_TOKEN="$CF_API_TOKEN_VALUE" pnpm exec wrangler --cwd "$(pwd)" deploy
CLOUDFLARE_ACCOUNT_ID="$CF_ACCOUNT_ID_VALUE" CLOUDFLARE_API_TOKEN="$CF_API_TOKEN_VALUE" pnpm exec wrangler --cwd "$(pwd)" r2 object put --remote "${PROJECT_NAME}-blog-bucket/article_snapshot_zst" -f "$WORKING_DIR/article_snapshot_new.zst"
CF_ACCOUNT_ID="$CF_ACCOUNT_ID_VALUE" CF_API_TOKEN="$CF_API_TOKEN_VALUE" ./send_to_queue.sh "${PROJECT_NAME}-job-queue" "$WORKING_DIR/events.jsonl"
