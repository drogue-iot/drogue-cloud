# Drogue IoT Cloud

[![CI](https://github.com/drogue-iot/drogue-cloud/workflows/CI/badge.svg)](https://github.com/drogue-iot/drogue-cloud/actions?query=workflow%3A%22CI%22)
[![GitHub release (latest SemVer)](https://img.shields.io/github/v/tag/drogue-iot/drogue-cloud?sort=semver)](https://github.com/drogue-iot/drogue-cloud/releases)
[![Matrix](https://img.shields.io/matrix/drogue-iot:matrix.org)](https://matrix.to/#/#drogue-iot:matrix.org)

> Serverless IoT.

The Drogue IoT Cloud takes care of the data on the cloud side.

![Overview diagram](images/architecture.svg)

It offers:

* IoT friendly protocol endpoints
* Protocol normalization based on Cloud Events and Knative eventing
* Managing of device credentials and properties
* APIs and a graphical console to manage devices and data flows

It is built on top of:

* *Kubernetes* – For running workloads
* *Cloud Events* - For normalizing transport protocols
* *Knative (serving & eventing)* – For offering endpoints and streaming data
* *Apache Kafka* – For persisting events
* *Keycloak* - For single-sign-on

## Installation

Take a look at the file [deploy/README.md](deploy/README.adoc), it should guide you through the process.

In a nutshell you need to:

~~~shell
minikube start --cpus 4 --memory 16384 --disk-size 20gb --addons ingress
minikube tunnel # in a separate terminal, as it keeps running
./hack/drogue.sh
~~~

## Building

See the document [CONTRIBUTING.md](CONTRIBUTING.md).
