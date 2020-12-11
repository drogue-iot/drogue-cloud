## Building

### In a container

You will need:

* GNU Make
* A container engine (e.g. Docker or Podman)
* An internet connection

To build and publish, run:

    make CONTAINER_REGISTRY=quay.io/your-org

The makefile will use a build container to perform the actual build.

## Deploy Helm charts of local components

### Drogue Cloud

~~~
helm install --dependency-update -n drogue-iot drogue-iot --set sources.mqtt.enabled=true --set services.console.enabled=true deploy/helm/drogue-iot --values deploy/helm/drogue-iot/profile-openshift.yaml
helm upgrade -n drogue-iot drogue-iot --set sources.mqtt.enabled=true --set services.console.enabled=true deploy/helm/drogue-iot --values deploy/helm/drogue-iot/profile-openshift.yaml
~~~


### Digital Twin

~~~
helm install --dependency-update -n drogue-iot digital-twin deploy/helm/digital-twin --values deploy/helm/digital-twin/profile-openshift.yaml
helm upgrade -n drogue-iot digital-twin deploy/helm/digital-twin --values deploy/helm/digital-twin/profile-openshift.yaml 
~~~
