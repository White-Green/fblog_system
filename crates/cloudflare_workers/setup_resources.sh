#!/usr/bin/env bash
set -euo pipefail

DATABASE_ID=$(pnpm exec wrangler d1 list --json | jq -r --arg DB "$DB_NAME" '.[] | select(.name == $DB) | .uuid')
if [ -z "$DATABASE_ID" ]; then
  echo "Create D1 Database '${{ inputs.project_name }}-blog-db'"
  pnpm exec wrangler d1 create "${{ inputs.project_name }}-blog-db"
  DATABASE_ID=$(pnpm exec wrangler d1 list --json | jq -r --arg DB "$DB_NAME" '.[] | select(.name == $DB) | .uuid')
fi

if [ -z "$DATABASE_ID" ]; then
  echo "ERROR: Cannot find D1 Database ${{ inputs.project_name }}-blog-db"
  exit 1
fi

PROJECT_NAME="${{ inputs.project_name }}" SITE_URL="${{ inputs.site_url }}" D1_DATABASE_ID=$DATABASE_ID envsubst < wrangler.template.toml > wrangler.toml
