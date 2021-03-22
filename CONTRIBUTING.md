
# Contributing

Thank you for your interest in the project and for considering contributing.

This guide should help you get started: creating a build and test environment, as well as contributing your work.

All contributions are welcome! While this guide will focus on contributing code, we would also encourage you to
contribute by reporting issues, providing feedback, suggesting new ideas. Or just by saying "hi" in the chat.

If you just want to run Drogue IoT cloud, take a look here: [deploy/README.md](deploy/README.adoc). That guide
should walk you through the installation of the stack. This guide is more of a "hacking guide", and assumes
that you plan to make changes, or at least compile your own version of the cloud components.

## Before you start

Before you start working on a fix or new feature, we would recommend to reach out to us and tell us about it. Maybe
we already have this in our heads (and forgot to create an issue for it), or maybe we have an alternative already.

In any case, it is always good to create an issue, or join the chat and tell us about your issues or plans. We will
definitely try to help you.

If you want to get started making changes to this project, you will need a few things. The following sub-sections
should help you get ready.

### Pre-requisites

In any case, you will need:

* Linux, Mac OS X, or Windows on an AMD64 platform (aka `x86_64`)
* Podman or Docker
  * Windows containers will not work, you need to use Linux based containers, and again `x86_64`. 
* Some tools
  * git
  * GNU Make
  * Python 3.x with PyYAML module
  * npm
  * kubectl
  * HTTPie 2.2+
* A lot of cores, patience, memory, and disk space
* Some form of Kubernetes cluster
  * **Minikube** is what seems to work best for development, and is easy to get started with.
  * **Kind** also works, uses less resources, but is less tested. 
  * **OpenShift** also works and make several things easier (like proper DNS names and certs), but is
    also more complex to set up.

### Optional requirements

* **Rust 1.49+** – By default the build will run inside a container image, with Rust included. So you don't necessarily
  need to install Rust on your local machine. However, having Rust installed might come in handy at some point. If you
  want to use an IDE, that might require a Rust installation. Or if you want to quickly run tests, maybe from inside
  your IDE, then this will require Rust as well.
  
  In any case, you need to be sure that you install at least the version of Rust mentioned above. If you installed
  Rust using `rustup` and default options, then performing an upgrade should be as easy as running `rustup update`.

* **An IDE** – Whatever works best for you. Eclipse, Emacs, IntelliJ, Vim, … [^1] should all be usable with this
  project. We do not require any specific IDE. We also do not commit any IDE specific files either.

[^1]: This list is sorted in alphabetical order, not in the order of any preference.

## Operating system

There are different ways to install the required dependencies on the different operating systems. Some operating
systems also might require some additional settings. This section should help to get you started.

### Fedora

Use an "update to date" version of Fedora. Install the following dependencies:

    sudo dnf install curl openssl-devel npm gcc gcc-c++ make cyrus-sasl-devel cmake libpq-devel postgresql podman podman-docker

### Windows

Assuming you have Windows 10 and admin access.

Install:

* Git for Windows
* GNU Make 4.x
  * install mingw-w64, as described here: https://code.visualstudio.com/docs/cpp/config-mingw
  * or, install "GNU make" using Chocolatey
* Docker for Windows
  * Enable WSL2

**FIXME:** Needs more testing

### Mac OS

Most of the required tools you can install using [brew](https://brew.sh/) package manager, e.g.

  brew install git make

Using OpenSSL and Cyrus SASL libraries native is still work in progress, so you should use container build for the time being
 as described below.

## Building

While the build is based on `cargo`, the build is still driven by the main `Makefile`, located in
the root of the repository. By default, the cargo build running inside a build container. This reduces
the number of pre-requisites you need to install, and makes it easier on platforms like Windows or Mac OS.

To perform a full build execute:

    make build

This builds the cargo based projects, the frontend, and the container images.

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

However, as tests are compiled and executed on the host machine, the same requirements, as when running
tests on the host machine, apply (see above).

## Publishing images

The locally built images can be published with the Makefile as well. For this you need a location to push to.
You can, for example use [quay.io](https://quay.io). Assuming your username on quay.io is "rodney", and
you did log in using `docker login`, then you could do:

    make push CONTAINER_REGISTRY=quay.io/rodney

## Deploying

By default, the installation scripts will use the official images from `ghcr.io/drogue-iot`.

When you created and published custom images, you can deploy them using `make` as well. Before you
do that, you will need to have access to a Kubernetes cluster. You can run a local cluster using `minikube`.
Make sure that your `minikube` cluster is started with `ingress` addon and that you run `tunnel` in a separate shell

~~~shell
minikube start --cpus 4 --memory 16384 --disk-size 20gb --addons ingress
minikube tunnel # in a separate terminal, as it keeps running
~~~

Once the instance is up, and you have ensured that you can access the cluster with `kubectl`, you can run
the following command to run the deployment:

    make deploy CONTAINER_REGISTRY=quay.io/rodney

## Contributing your work

Thank you for reading the document up to this point and for taking the next step.

### Pre-flight check

Before creating a pull-request (PR), you should do some pre-flight checks, which the CI will run later on anyway.
Running locally will give you quicker results, and safe us a bit of time and CI resources.

It is as easy as running:

    make check

This will:

* Check source code formatting
* Run `cargo check`
* Run `cargo clippy`

The `clippy` checks should be seen as *suggestions*. Take a look at them, in some cases you will learn something new. If
it sounds reasonable, it might be wise to fix it. Maybe it flags files you didn't even touch. In this case just ignore
them, was we might not have fixed all the clippy suggestions ourselves.

### Creating a PR

Nothing fancy, just a normal PR. The CI will be triggered and come back with results. People tend to pay more attention
to PRs that show up "green". So maybe check back and ensure that the CI comes up "green" for your PR as well. If it
doesn't, and you don't understand why, please reach out to us.

There are bonus points for adding your own tests ;-)

## How to …

### … work on the frontend

You will need to have `npm` installed, as it will drive parts of the build.

* Start the console backend locally
  * Set `CLIENT_ID` to `drogue`
  * Set `CLIENT_SECRET` to the value of `kubectl get secret keycloak-client-secret-drogue -o jsonpath='{.data[\'CLIENT_SECRET\']}' | base64 -d`
* Run the console:
  ~~~
  cd console-frontend
  npm run start:dev
  ~~~
