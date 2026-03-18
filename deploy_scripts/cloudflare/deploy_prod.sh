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

HOST_NAME=$(node -e 'const i=process.argv[1]; console.log(new URL(/^https?:\/\//.test(i) ? i : `https://${i}`).hostname);' "$SITE_URL")

"${SCRIPT_DIR}/../build.sh" \
  "$ARTICLE_DATA_PATH" \
  "$USER_DATA_PATH" \
  "$BUILD_DATA_PATH" \
  "$SITE_URL" \
  "$PUBLIC_KEY_PATH"

cd "$WORKING_DIR"

./crates/cloudflare_workers/update_snapshot.sh "$PROJECT_NAME"
pnpm exec wrangler r2 object put --remote "${PROJECT_NAME}-blog-bucket/article_snapshot.zst" -f ./article_snapshot_new.zst

mv dist crates/cloudflare_workers/public
cat events.jsonl

cd crates/cloudflare_workers
./setup_resources.sh "$PROJECT_NAME" "$HOST_NAME"
pnpm exec wrangler deploy
CF_ACCOUNT_ID="$CLOUDFLARE_ACCOUNT_ID" CF_API_TOKEN="$CLOUDFLARE_API_TOKEN" ./send_to_queue.sh "${PROJECT_NAME}-job-queue" "$WORKING_DIR/events.jsonl"
