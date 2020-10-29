This is a simple end-to-end test, publishing data via HTTP or MQTT, delivering with Kafka, to a Grafana dashboard.

## How to replicate

Take a look at the file [deploy/README.md](deploy/README.adoc). It guides you through the steps to replicate this.

## Building

You will need:

* GNU Make
* A container engine (e.g. Docker or Podman)
* An internet connection

To build and publish, run:

    make CONTAINER_REGISTRY=quay.io/your-org
