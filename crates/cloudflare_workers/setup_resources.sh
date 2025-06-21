#!/usr/bin/env bash
set -euo pipefail

PROJECT_NAME="$1"
SITE_URL="$2"
DB_NAME="$PROJECT_NAME-blog-db"
BUCKET_NAME="$PROJECT_NAME-blog-bucket"
QUEUE_NAME="$PROJECT_NAME-job-queue"

DATABASE_ID=$(pnpm exec wrangler d1 list --json | jq -r --arg DB "$DB_NAME" '.[] | select(.name == $DB) | .uuid')
if [ -z "$DATABASE_ID" ]; then
  echo "Create D1 Database '$DB_NAME'"
  pnpm exec wrangler d1 create "$DB_NAME"
  DATABASE_ID=$(pnpm exec wrangler d1 list --json | jq -r --arg DB "$DB_NAME" '.[] | select(.name == $DB) | .uuid')
fi

if [ -z "$DATABASE_ID" ]; then
  echo "ERROR: Cannot find D1 Database $DB_NAME"
  exit 1
fi

BUCKET_EXISTS=$(pnpm exec wrangler r2 bucket list --json | jq -r --arg BUCKET "$BUCKET_NAME" '.[] | select(.name == $BUCKET) | .name')
if [ -z "$BUCKET_EXISTS" ]; then
  echo "Create R2 Bucket '$BUCKET_NAME'"
  pnpm exec wrangler r2 bucket create "$BUCKET_NAME"
fi

QUEUE_EXISTS=$(pnpm exec wrangler queues list --json | jq -r --arg QUEUE "$QUEUE_NAME" '.[] | select(.name == $QUEUE) | .name')
if [ -z "$QUEUE_EXISTS" ]; then
  echo "Create Queue '$QUEUE_NAME'"
  pnpm exec wrangler queues create "$QUEUE_NAME"
fi

PROJECT_NAME="$PROJECT_NAME" SITE_URL="$SITE_URL" D1_DATABASE_ID=$DATABASE_ID envsubst < wrangler.template.toml > wrangler.toml

echo "Resources setup completed successfully!"
