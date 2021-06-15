#!/bin/bash
LOGDIR=$1
NAMESPACE=$2

if which kubectl &> /dev/null; then
    CMD="kubectl -n ${NAMESPACE}"
else
    >&2 echo "$0: Cannot find kubectl command, please check path to ensure it is installed"
    exit 1
fi


function runcmd {
    local cmd=$1
    local logfile=$2
    echo "$cmd > $logfile"
    ${cmd} > "${logfile}"
}

#extract overall status
mkdir -p "${LOGDIR}/logs/"
runcmd "${CMD} get all" "${LOGDIR}/logs/all.log"

#extract the pods logs
mkdir -p "${LOGDIR}/logs/pods/"

for pod in $(${CMD} get pods -o jsonpath='{.items[*].metadata.name}')
do
    for container in $(${CMD} get pod "$pod" -o jsonpath='{.spec.containers[*].name}')
    do
        runcmd "${CMD} logs -c $container $pod" "${LOGDIR}/logs/pods/${pod}_${container}.log"
    done
    runcmd "${CMD} describe pod $pod" "${LOGDIR}/logs/pods/${pod}_describe.log"
done

function gather() {
  local resource=$1
  shift

  mkdir -p "${LOGDIR}/logs/${resource}/"

  for item in $(${CMD} get "${resource}" -o jsonpath='{.items[*].metadata.name}')
  do
      runcmd "${CMD} describe ${resource} ${item}" "${LOGDIR}/logs/${resource}/${item}.log"
      runcmd "${CMD} get ${resource} ${item} -o yaml" "${LOGDIR}/logs/${resource}/${item}.yaml"
  done
}

# Kubernetes

gather "nodes"
gather "services"
gather "deployments"
gather "secrets"
gather "configmaps"

# Keycloak

gather "keycloaks"
gather "keycloakrealms"
gather "keycloakclients"
gather "keycloakusers"

# Kafka

gather "kafkas"
gather "kafkatopics"

# Knative serving

gather "ksvc"
gather "revisions"
gather "cfg"

# Knative eventing

gather "sinkbindings"
gather "kafkasources"
gather "kafkasinks"
