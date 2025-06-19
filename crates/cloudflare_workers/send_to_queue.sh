#!/usr/bin/env bash

set -eu

QUEUE_NAME=$1
DATA_PATH=$2

QUEUE_ID=$(curl -s -H "Authorization: Bearer $CF_API_TOKEN" "https://api.cloudflare.com/client/v4/accounts/$CF_ACCOUNT_ID/queues" | jq -r --arg Q "$QUEUE_NAME" '.result[]|select(.queue_name==$Q)|.queue_id')
DATA=$(jq -Rs 'split("\n")[:-1] | map({body:(fromjson)}) | {messages:.}' "$DATA_PATH")

curl -X POST -H "Authorization: Bearer $CF_API_TOKEN" -H "Content-Type: application/json" --data "$DATA" "https://api.cloudflare.com/client/v4/accounts/$CF_ACCOUNT_ID/queues/$QUEUE_ID/messages/batch"
