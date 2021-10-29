#!/bin/bash

echo "Migrating database"
podman  run -e DATABASE_URL=postgres://admin:admin123456@localhost:5432/drogue -e POSTGRES_DB=drogue -e POSTGRES_USER=admin -e POSTGRES_PASSWORD=admin123456 --net=host ghcr.io/drogue-iot/database-migration:latest

echo "Setting up keycloak"
