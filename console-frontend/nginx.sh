#!/usr/bin/env bash

: ${BACKEND_URL:=http://localhost:8081}

echo "{\"url\":\"$BACKEND_URL\"}" > /endpoints/backend.json

/usr/sbin/nginx -g "daemon off;"
