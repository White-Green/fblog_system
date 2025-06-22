#!/usr/bin/env bash
# Usage: snapshot_diff.sh <old> <new>

set -eu

OLD="$1"
NEW="$2"

join -t $'\t' -a1 -a2 -e MISSING -o 0 1.2 1.3 2.3 \
     <(sort "$OLD") <(sort "$NEW") |
awk -F'\t' '
  $3=="MISSING"             {print "{\"event_type\":\"DeliveryNewArticleToAll\",\"slug\":\""$1"\"}"; next}
  $4=="MISSING"             {print "{\"event_type\":\"DeliveryDeleteArticleToAll\",\"slug\":\""$1"\",\"author\":\""$2"\"}"; next}
  $3 != $4                  {print "{\"event_type\":\"DeliveryUpdateArticleToAll\",\"slug\":\""$1"\"}"; next}
'
