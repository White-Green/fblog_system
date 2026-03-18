#!/usr/bin/env bash

set -eu

QUEUE_NAME=$1
DATA_PATH=$2
BATCH_SIZE=50

QUEUE_ID=$(curl -s -H "Authorization: Bearer $CF_API_TOKEN" "https://api.cloudflare.com/client/v4/accounts/$CF_ACCOUNT_ID/queues" | jq -r --arg Q "$QUEUE_NAME" '.result[]|select(.queue_name==$Q)|.queue_id')
TOTAL_LINES=$(jq -Rs 'split("\n") | map(select(length > 0)) | length' "$DATA_PATH")

if [ "$TOTAL_LINES" -eq 0 ]; then
  exit 0
fi

for (( offset=0; offset<TOTAL_LINES; offset+=BATCH_SIZE )); do
  DATA=$(jq -Rs --argjson offset "$offset" --argjson limit "$BATCH_SIZE" 'split("\n") | map(select(length > 0)) | .[$offset:($offset + $limit)] | map({body:(fromjson)}) | {messages:.}' "$DATA_PATH")

  curl -X POST -H "Authorization: Bearer $CF_API_TOKEN" -H "Content-Type: application/json" --data "$DATA" "https://api.cloudflare.com/client/v4/accounts/$CF_ACCOUNT_ID/queues/$QUEUE_ID/messages/batch"
done
