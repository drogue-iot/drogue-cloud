
# Drogue IoT Cloud

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
./script/drogue.sh
~~~

## Building

See the document [HACKING.md](HACKING.md).
