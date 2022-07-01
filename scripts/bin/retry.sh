#!/usr/bin/env bash

: "${MAX_RETRY:=500}"
: "${WAIT:=10}"

: "${CONTAINER:=docker}"
RETRY=0

echo "Run with retry: $CONTAINER " "$@"

while (( RETRY < MAX_RETRY )); do
  echo "Pushing image - attempt $RETRY of $MAX_RETRY"
  (( RETRY ++ ))
  "$CONTAINER" "$@" && exit 0
  echo "Failed to execute ... sleeping: $WAIT seconds"
  sleep "$WAIT"
done

echo "Completed: $CONTAINER " "$@"
