#!/usr/bin/env bash
set -euo pipefail

PROJECT_NAME="$1"
HOST_NAME="$2"
DB_NAME="$PROJECT_NAME-blog-db"
BUCKET_NAME="$PROJECT_NAME-blog-bucket"
QUEUE_NAME="$PROJECT_NAME-job-queue"

if pnpm exec wrangler d1 info "$DB_NAME" > /dev/null 2>&1; then
  echo "D1 Database '${DB_NAME}' already exists"
else
  echo "Create D1 Database '${DB_NAME}'"
  pnpm exec wrangler d1 create "$DB_NAME"
fi

DATABASE_ID=$(pnpm exec wrangler d1 info "$DB_NAME" --json | jq -r '.uuid')

if [ -z "$DATABASE_ID" ]; then
  echo "ERROR: Cannot find D1 Database $DB_NAME"
  exit 1
fi

if pnpm exec wrangler r2 bucket info "$BUCKET_NAME" > /dev/null 2>&1; then
  echo "R2 Bucket '${BUCKET_NAME}' already exists"
else
  echo "Create R2 Bucket '${BUCKET_NAME}'"
  pnpm exec wrangler r2 bucket create "$BUCKET_NAME"
fi

if pnpm exec wrangler queues info "$QUEUE_NAME" > /dev/null 2>&1; then
  echo "Queue '${QUEUE_NAME}' already exists"
else
  echo "Create Queue '${QUEUE_NAME}'"
  pnpm exec wrangler queues create "$QUEUE_NAME"
fi

PROJECT_NAME="$PROJECT_NAME" HOST_NAME="$HOST_NAME" D1_DATABASE_ID=$DATABASE_ID envsubst < wrangler.template.toml > wrangler.toml

cat wrangler.toml
ls migrations

pnpm exec wrangler d1 migrations apply --config "$(pwd)/wrangler.toml" --remote "$DB_NAME"

echo "Resources setup completed successfully!"
