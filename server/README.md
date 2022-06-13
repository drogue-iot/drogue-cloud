# Drogue Server

A single binary for running all Drogue Cloud services.

## Building

### Dependencies

To build drogue-server, you need:

* cyrus SASL
* librdkafka
* openssl
* libpq

#### Installing dependencies on Fedora

```
dnf install gcc-c++ cyrus-sasl-devel openssl-devel libpq-devel librdkafka-devel
```

#### Installing dependencies on Mac OS X

```
brew install cyrus2-sasl librdkafka openssl libpq
```

### Building the binary

On a Linux-based host, `cargo build` should "just work".

On Mac OS X, the following command to make libraries known to Rust has been tested to work:
```
RUSTFLAGS='-L /opt/homebrew/opt/libpq/lib' PKG_CONFIG_PATH=/opt/homebrew/opt/libpq/lib/pkgconfig:/opt/homebrew/opt/openssl/lib/pkgconfig:/opt/homebrew/opt/cyrus-sasl/lib/pkgconfig:/opt/homebrew/opt/librdkafka/lib/pkgconfig cargo build --release
```

## Dependencies

To run drogue server, you need to have running instances of PostgreSQL, Kafka and Keycloak.

### Podman/Docker compose

If you're on a host with docker-compose or podman-compose, you can simply run the following command in
this folder:

```shell
podman-compose up
```

### Podman play

Using podman, you can also start some Kubernetes pods locally, with `podman play` and without an actual Kubernetes
installation:

```shell
podman play kube kube-play.yaml
```

Stop the containers using:

```shell
podman play kube kube-play.yaml --down
```

Update existing:

```shell
podman play kube kube-play.yaml --replace
```

## Running the server

You can run the server with `--help` to discover how to run the server, but the simplest way is to
run:

```shell
./target/release/drogue-cloud-server run --enable-all
```

This start the drogue services and print some useful information on how to connect.

You can also use `cargo` to compile and run the server from this folder:

```shell
cargo run -- run --enable-all
```
