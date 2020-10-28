# Build

FROM registry.access.redhat.com/ubi8/ubi:latest as builder

RUN dnf -y update
RUN dnf -y install curl openssl-devel npm gcc gcc-c++ make cyrus-sasl-devel

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

ENV PATH "$PATH:/root/.cargo/bin"

RUN rustup target add wasm32-unknown-unknown

RUN cargo install wasm-pack
ENV WASM_PACK_PATH "/root/.cargo/bin/wasm-pack"
RUN cargo install wasm-bindgen-cli

RUN mkdir -p /usr/src
VOLUME /usr/src/

ENV CARGO_HOME "/usr/src/.cargo-container-home"
