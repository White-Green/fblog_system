#!/usr/bin/env bash
set -euo pipefail

PROJECT_NAME="$1"
SITE_URL="$2"
DB_NAME="$PROJECT_NAME-blog-db"

DATABASE_ID=$(pnpm exec wrangler d1 list --json | jq -r --arg DB "$DB_NAME" '.[] | select(.name == $DB) | .uuid')
if [ -z "$DATABASE_ID" ]; then
  echo "Create D1 Database '$PROJECT_NAME-blog-db'"
  pnpm exec wrangler d1 create "$PROJECT_NAME-blog-db"
  DATABASE_ID=$(pnpm exec wrangler d1 list --json | jq -r --arg DB "$DB_NAME" '.[] | select(.name == $DB) | .uuid')
fi

if [ -z "$DATABASE_ID" ]; then
  echo "ERROR: Cannot find D1 Database $PROJECT_NAME-blog-db"
  exit 1
fi

PROJECT_NAME="$PROJECT_NAME" SITE_URL="$SITE_URL" D1_DATABASE_ID=$DATABASE_ID envsubst < wrangler.template.toml > wrangler.toml
