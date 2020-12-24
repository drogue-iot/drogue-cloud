#!/usr/bin/env bash

if [ -n "$IMAGE_REPO" ] || [ -n "$IMAGE_ORG" ] || [ -n "$IMAGE_TAG" ]; then
    : "${IMAGE_REPO:=ghcr.io}"
    : "${IMAGE_ORG:=drogue-iot}"
    : "${IMAGE_TAG:=latest}"

    (cd deploy/base/source/http && kustomize edit set image ghcr.io/drogue-iot/http-endpoint=$IMAGE_REPO/$IMAGE_ORG/http-endpoint:$IMAGE_TAG)
    (cd deploy/base/source/mqtt && kustomize edit set image ghcr.io/drogue-iot/mqtt-endpoint=$IMAGE_REPO/$IMAGE_ORG/http-endpoint:$IMAGE_TAG)
fi