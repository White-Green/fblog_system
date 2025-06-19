#!/usr/bin/env bash
# Usage: snapshot.sh [Directory] > snapshot.tsv
# Output format:  <slug>\t<author>\t<SHA-256>

set -eu

TARGET_DIR="${1:-.}"
cd "$TARGET_DIR"

find . -type f -print0 | sort -z |
while IFS= read -r -d '' path; do
  author=$(jq -r '.attributedTo' "$path")
  hash=$(cat "$path" | jq -Sc 'del(.updated)' | sha256sum | cut -d' ' -f1)
  path="${path#./}"
  printf '%s\t%s\t%s\n' "${path%.json}" "${author##*/}" "$hash"
done
