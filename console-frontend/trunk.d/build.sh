#!/usr/bin/env bash

set -e

echo "Trunk profile: $TRUNK_PROFILE"

if [[ "$TRUNK_PROFILE" == "debug" ]]; then
    echo "Copy dev folder to staging: $TRUNK_STAGING_DIR"
    cp -a dev/endpoints "$TRUNK_STAGING_DIR"

    # check if we have a local file, which will override
    if [[ -f "$TRUNK_STAGING_DIR/endpoints/backend.local.json" ]]; then
        echo "Override with local settings..."
        cp "$TRUNK_STAGING_DIR/endpoints/backend.local.json" "$TRUNK_STAGING_DIR/endpoints/backend.json"
    fi
fi
