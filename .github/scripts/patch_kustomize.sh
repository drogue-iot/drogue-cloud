#!/bin/bash
SHA=$1
ORG=$2

sed -i "s/REPLACE-TAG/${SHA}/" ./.github/resources/ci_kustomize_images.yaml
sed -i "s/REPLACE-ORG/${ORG}/" ./.github/resources/ci_kustomize_images.yaml
cat ./.github/resources/ci_kustomize_images.yaml >> ./deploy/kind/kustomization.yaml

cp ./.github/resources/image_pull_policy.yaml ./deploy/kind/
