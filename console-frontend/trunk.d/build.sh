#!/usr/bin/env bash

set -e

# echo "Trunk profile: $TRUNK_PROFILE"

if [[ "$TRUNK_PROFILE" == "debug" ]]; then
    echo "Copy dev folder to staging: $TRUNK_STAGING_DIR"
    cp -a dev/endpoints "$TRUNK_STAGING_DIR"
fi