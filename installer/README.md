## Installation

Download one of the installers, extract and run the installer script `scripts/drgadm` from the main directory of
the archive.

You will need:

* Bash
* `kubectl`
* `curl`
* `helm`
* Podman or docker
* A Kubernetes cluster (also see below)

By default, the cluster type will be aligned with the downloaded installer. However, you can override this using
the `CLUSTER` variable:

~~~shell
env CLUSTER=kind ./scripts/drgadm deploy
~~~

### Minikube

* Install Minikube – https://minikube.sigs.k8s.io/docs/start/

~~~shell
minikube start --cpus 4 --memory 16384 --disk-size 20gb --addons ingress
minikube tunnel # in a separate terminal, as it keeps running
./scripts/drgadm deploy
~~~

### Kind

* Install `kind` – https://github.com/kubernetes-sigs/kind/releases

~~~shell
kind create cluster --config=deploy/kind/cluster-config.yaml
./scripts/drgadm deploy
~~~
