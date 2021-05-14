# Release cheat sheet

## Next version

Preparing for a new version (not release, like a milestone):

* Change the version in all crates to e.g. `0.4.0`
  * Pay attention to the `service-api` crate as its version will be reported externally

## Overall process

* Create a new tag
  * Start with a `v0.x.0-rc1` version
  * The final version should be `v0.x.0`
* Push the tag
* Wait for the build
* Test the instructions in the following "Installation" subsections
* For each installation:
  * Test the links on the command line
  * Test the links in the web console
  * Try out the example commands
* Create a branch `release-0.x`
  * Ensure to switch the doc version to 0.x too: `docs/antora.yml`

## Release text

The text that goes into the final GitHub release record:

---

## Installation

Download one of the installers, extract and run the installer script `scripts/drogue.sh` from the main directory of
the archive.

You will need:

  * Bash
  * `kubectl`
  * `curl`
  * Podman or docker
  * A Kubernetes cluster (also see below)

By default, the cluster type will be aligned with the downloaded installer. However, you can override this using
the `CLUSTER` variable:

~~~shell
env CLUSTER=kind ./scripts/drogue.sh
~~~

### Minikube

* Install Minikube – https://minikube.sigs.k8s.io/docs/start/

~~~shell
minikube start --cpus 4 --memory 16384 --disk-size 20gb --addons ingress
minikube tunnel # in a separate terminal, as it keeps running
./scripts/drogue.sh
~~~

### Kind

* Install `kind` – https://github.com/kubernetes-sigs/kind/releases

~~~shell
kind create cluster
./scripts/drogue.sh
~~~


---
