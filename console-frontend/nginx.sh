#!/usr/bin/env bash

: "${BACKEND_URL:=http://localhost:8011}"

echo "Setting backend endpoint:"

echo '{}' | jq --arg url "$BACKEND_URL" '. + {url: $url}' | tee /endpoints/backend.json

exec /usr/sbin/nginx -g "daemon off;"
