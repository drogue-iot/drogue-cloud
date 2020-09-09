# Deploying

## Create a new namespace

    oc new-project drogue-iot

**Note:** Currently some parts of the deployment actually expect the name `drogue-iot`. If you change it, you will
break things.

## Create tekton pipeline

    oc apply -f deploy/01-build

## Create a build workspace claim

You will need a workspace for running the build pipeline. It is expected to be a persistent volume claim (PVC) with
the name of `build-workspace`.

You can create one with the following command:

    oc apply -f deploy/build-pvc.yaml

The claim can be reused for builds. Of course, it can also be destroyed and re-created.

## Start a new build

    tkn pipeline start build-drogue-cloud --showlog -p repo-owner=ctron --workspace name=shared-data,claimName=build-workspace
