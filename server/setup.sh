#!/bin/bash

echo "Setting up keycloak"
docker exec server_keycloak_1 bash -c '/opt/jboss/keycloak/bin/kcadm.sh config credentials --server http://localhost:8080/auth --realm master --user admin --password admin123456'
docker cp ./drogue-client.json server_keycloak_1:/tmp/drogue-client.json
docker exec server_keycloak_1 bash -c '/opt/jboss/keycloak/bin/kcadm.sh create clients -f /tmp/drogue-client.json' 
