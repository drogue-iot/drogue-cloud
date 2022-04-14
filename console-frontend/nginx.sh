#!/usr/bin/env bash

set -e
set -x
set -o pipefail

: "${API_URL:=http://localhost:8011}"
: "${ISSUER_URL:=http://localhost:8012}"
: "${CLIENT_ID:=drogue}"
: "${BACKEND_JSON:="{}"}"
: "${BACKEND_JSON_FILE:=/etc/config/login/backend.json}"

echo "Setting backend endpoint:"

if [ -f "$BACKEND_JSON_FILE" ]; then
    echo "Using base config from file: $BACKEND_JSON_FILE"
    BACKEND_JSON="$(cat "$BACKEND_JSON_FILE")"
fi

# inject backend URL
echo "$BACKEND_JSON" | jq --arg url "$API_URL" '. + {url: $url}' | tee /endpoints/backend.json

# inject oauth2 information
jq --arg url "$CLIENT_ID" '.openid += {client_id: $url}' < /endpoints/backend.json | tee /endpoints/backend.json.tmp
mv /endpoints/backend.json.tmp /endpoints/backend.json
jq --arg url "$ISSUER_URL" '.openid += {issuer_url: $url}' < /endpoints/backend.json | tee /endpoints/backend.json.tmp
mv /endpoints/backend.json.tmp /endpoints/backend.json

LOGIN_NOTE=/etc/config/login/note.html
if [ -f "$LOGIN_NOTE" ]; then
  echo "Adding login note: $LOGIN_NOTE"
  jq --arg note "$(cat "$LOGIN_NOTE")" '. + {login_note: $note}' < /endpoints/backend.json | tee /endpoints/backend.json.tmp
  mv /endpoints/backend.json.tmp /endpoints/backend.json
else
  echo "Skipping login note: $LOGIN_NOTE"
fi

echo "Final backend information:"
echo "---"
cat /endpoints/backend.json
echo "---"

exec /usr/sbin/nginx -g "daemon off;"
