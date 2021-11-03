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

## Running

### Dependencies

To run drogue server, you need to have running instances of PostgreSQL, Kafka and Keycloak. If
you're on a host with docker-compose or podman-compose, you can simply run the following command in
this folder:

```
podman-compose up
```

## Running the server

You can run the server with `--help` to discover how to run the server, but the simplest way is to
run:

```
./target/release/drogue-cloud-server run --enable-all
```

This start the drogue services and print some useful information on how to connect.

You can also use `cargo run -- run --enable-all` to run the server from this folder.

