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
