# Drogue IoT Cloud

[![CI](https://github.com/drogue-iot/drogue-cloud/workflows/CI/badge.svg)](https://github.com/drogue-iot/drogue-cloud/actions?query=workflow%3A%22CI%22)
[![GitHub release (latest SemVer)](https://img.shields.io/github/v/tag/drogue-iot/drogue-cloud?sort=semver)](https://github.com/drogue-iot/drogue-cloud/releases)
[![Matrix](https://img.shields.io/matrix/drogue-iot:matrix.org)](https://matrix.to/#/#drogue-iot:matrix.org)

> Cloud Native IoT.

Drogue IoT Cloud is an IoT/Edge connectivity layer that allows IoT devices to communicate with a cloud platform over
various protocols. It acts both as data ingestion plane, and as control plane. In short, Drogue IoT Cloud takes
care of the data on the cloud side :grin:.

![Overview diagram](docs/modules/ROOT/images/architecture.svg)

It offers:
* IoT friendly protocol endpoints and APIs
* Protocol normalization based on Cloud Events
* Management of device credentials and properties
* APIs, a CLI, and a graphical console to manage devices and data flows

It is built on top of:
* *Kubernetes* – For running workloads
* *Cloud Events* - For normalizing transport protocols
* *Knative (eventing)* – For streaming data
* *Apache Kafka* – For persisting events
* *Keycloak* - For single-sign-on

You can learn more about the [architecture](https://book.drogue.io/drogue-cloud/dev/architecture/index.html) in
our [documentation](https://book.drogue.io/).

## Protocol Endpoint Support

| Protocols                  |     Endpoint    |
| -------------------------- | :-------------: |
| HTTP                       |        ✓        |
| MQTT v3/v5                 |        ✓        |
| CoAP                       |  Coming soon    |

## Installation

Take a look at the [deployment instructions](https://book.drogue.io/drogue-cloud/dev/deployment/).

If you know what you are doing, the next sections give you a quick may of deploying the "latest" version of Drogue Cloud.

### Minikube

~~~shell
minikube start --cpus 4 --memory 16384 --disk-size 20gb --addons ingress
minikube tunnel # run this in a separate terminal, as it needs to keep running
make deploy CLUSTER=minikube
~~~

### Kind

~~~shell
kind create cluster --config=deploy/kind/cluster-config.yaml
make deploy CLUSTER=kind
~~~

## Useful Links

* [Documentation](https://book.drogue.io/drogue-cloud/dev/index.html)
* [Drogue IoT Blog: Articles that talk about the design,  use-cases and project updates](https://blog.drogue.io/)

## Contributing

See the document [CONTRIBUTING.md](CONTRIBUTING.md).

## Community

* [Drogue IoT Matrix Chat Room](https://matrix.to/#/#drogue-iot:matrix.org)
* We have bi-weekly calls at 9:00 AM (GMT). [Check the calendar](https://calendar.google.com/calendar/u/0/embed?src=ofuctjec399jr6kara7n0uidqg@group.calendar.google.com&pli=1) to see which week we are having the next call, and feel free to join!
* [Drogue IoT Forum](https://discourse.drogue.io/)
* [Drogue IoT YouTube channel](https://www.youtube.com/channel/UC7GZUy2hKidvY6V_3QZfCcA)
* [Follow us on Twitter!](https://twitter.com/DrogueIoT)
