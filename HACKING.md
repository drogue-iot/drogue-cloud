
## Pre-requisites

You will need:

* Linux, Mac OS X, or Windows
* Podman or Docker
* Some tools
  * Make
  * npm
  * kubectl
* A lot of cores, patience, memory, and disk space

### Fedora

Use an "update to date" version of Fedora. Install the following dependencies:

    dnf install curl openssl-devel npm gcc gcc-c++ make cyrus-sasl-devel cmake libpq-devel kubectl podman podman-docker

## Building

While the build is based on `cargo`, the build is still driven by the main `Makefile`, located in
the root of the repository. By default, the cargo build running inside a build container. This reduces
the number of pre-requisites you need to install, and makes it easier on platforms like Windows or Mac OS.

To perform a full build execute:

    make

This builds the cargo based projects, the frontend, and the container images. It does not tag and push
the images.

## Testing

To run all tests:

    make test

**Note:** When using podman, you currently cannot use `make test`. You need to revert
to `make container-test`, see below.

### Running test on the host

If you have a full build environment on your machine, you can also execute the tests on the host machine,
rather than forking them off in the build container:

    make container-test

### IDE based testing

You can also run cargo tests directly from your IDE. How this works, depends on your IDE.

However, as tests are compiled and executed on the host machine, the same requirements as when running
tests on the host machine apply (see above).

## Publishing images

The locally built images can be published with the Makefile as well. For this you need a location to push to.
You can, for example use [quay.io](https://quay.io). Assuming your username on quay.io is "rodney", and
you did log in using `docker login`, then you could do:

    make push CONTAINER_REGISTRY=quay.io/rodney

## Deploying

By default, the installation scripts will use the official images from `ghcr.io/drogue-iot`.

However, when you created and published custom images, you can deploy them using `make` as well. Before you
do that, you will need to have access to a Kubernetes cluster. You can run a local cluster using `minikube`.
Once the instance is up, and you have ensured that you can access the cluster with `kubectl`, you can run
the following command to run the deployment:

    make deploy CONTAINER_REGISTRY=quay.io/rodney
