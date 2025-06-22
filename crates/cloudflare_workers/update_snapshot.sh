#!/usr/bin/env bash

set -eu

PROJECT_NAME=$1

if pnpm exec wrangler r2 object get --remote "$PROJECT_NAME-blog-bucket/article_snapshot.zst" -f ./article_snapshot_old.zst; then
  zstd -d ./article_snapshot_old.zst -o ./article_snapshot_old
  cat ./article_snapshot_old
else
  cat /dev/null > article_snapshot_old
fi

mkdir -p dist/raw__/articles/ap
./snapshot.sh dist/raw__/articles/ap > article_snapshot_new
./snapshot_diff.sh ./article_snapshot_old ./article_snapshot_new > events.jsonl
zstd -f -19 article_snapshot_new -o article_snapshot_new.zst
