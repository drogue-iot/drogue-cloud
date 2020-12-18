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
    ${cmd} > ${logfile}
}

#extract the pods logs
mkdir -p ${LOGDIR}/logs/pods/

for pod in `${CMD} get pods -o jsonpath='{.items[*].metadata.name}'`
do
    for container in `${CMD} get pod $pod -o jsonpath='{.spec.containers[*].name}'`
    do
        runcmd "${CMD} logs -c $container $pod" ${LOGDIR}/logs/pods/${pod}_${container}.log
        runcmd "${CMD} describe pod $pod" ${LOGDIR}/logs/pods/${pod}_describe.log
    done
done

#extract the deployment logs
mkdir -p ${LOGDIR}/logs/deployments/

for deploy in `${CMD} get deployments -o jsonpath='{.items[*].metadata.name}'`
do
    runcmd "${CMD} describe deployment $deploy " ${LOGDIR}/logs/deployments/${deploy}.log
    runcmd "${CMD} get deployment $deploy -o yaml" ${LOGDIR}/logs/deployments/${deploy}.yaml
done

#extract the ksvc logs
mkdir -p ${LOGDIR}/logs/ksvc/

for ksvc in `${CMD} get ksvc -o jsonpath='{.items[*].metadata.name}'`
do
    runcmd "${CMD} describe ksvc $ksvc" ${LOGDIR}/logs/ksvc/${ksvc}.log
done

#extract the services logs
mkdir -p ${LOGDIR}/logs/services/

for svc in `${CMD} get services -o jsonpath='{.items[*].metadata.name}'`
do
    runcmd "${CMD} describe service $svc" ${LOGDIR}/logs/services/${svc}.log
done

#extract the node resource usage
mkdir -p ${LOGDIR}/logs/nodes/

for node in `${CMD} get nodes -o jsonpath='{.items[*].metadata.name}'`
do
    runcmd "${CMD} describe node $node" ${LOGDIR}/logs/nodes/${node}.log
done
