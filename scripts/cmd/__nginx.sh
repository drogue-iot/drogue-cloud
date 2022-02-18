#!/usr/bin/env bash

set -e

: "${NGINX_VERSION:=1.1.0}"

echo "Installing NGINX Ingress Controller: ${NGINX_VERSION}"
progress "📦 Deploying pre-requisites (NGINX Ingress Controller v${NGINX_VERSION}) ... "

progress -n "  🏗 Deploying operator ... "
kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/controller-v${NGINX_VERSION}/deploy/static/provider/kind/deploy.yaml
progress "done!"

if [[ "$MINIMIZE" == true ]]; then
    kubectl -n ingress-nginx set resources deployment ingress-nginx-controller --requests=cpu=0
fi

progress -n "  ⏳ Waiting for the operator to become ready ... "
kubectl wait --namespace ingress-nginx --for=condition=Available deployment ingress-nginx-controller
progress "done!"
