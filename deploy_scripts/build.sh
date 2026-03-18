#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKING_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARTICLE_DATA_PATH="$1"
USER_DATA_PATH="$2"
BUILD_DATA_PATH="$3"
SITE_URL="$4"
PUBLIC_KEY_PATH="$5"
MOVE_HTML_TO_RAW="$6"

case "$MOVE_HTML_TO_RAW" in
  true|false) ;;
  *)
    echo "[fblog_system] ERROR: MOVE_HTML_TO_RAW must be true or false" >&2
    exit 1
    ;;
esac

HOST_NAME=$(node -e 'const i=process.argv[1]; console.log(new URL(/^https?:\/\//.test(i) ? i : `https://${i}`).hostname);' "$SITE_URL")

cd "$WORKING_DIR"

mkdir -p contents/articles contents/users public
cp -r "$ARTICLE_DATA_PATH"/. contents/articles/
cp -r "$USER_DATA_PATH"/. contents/users/
cp -r "$BUILD_DATA_PATH"/. public/

if [ "$MOVE_HTML_TO_RAW" = "true" ]; then
  for dir in articles users; do
    mkdir -p "public/raw__/${dir}/html" "public/${dir}"
    find "public/${dir}" -mindepth 1 -maxdepth 1 ! -name index.html -exec mv -t "public/raw__/${dir}/html" {} + || true
    cd "public/raw__/${dir}/html"
    find . -type f -name index.html -print0 | while IFS= read -r -d '' file; do mv "$file" "${file%/*}.html"; done
    cd - >/dev/null
  done
fi

pnpm install
PUBLIC_KEY_FILE="$PUBLIC_KEY_PATH" SITE_URL="https://${HOST_NAME}" pnpm run build
