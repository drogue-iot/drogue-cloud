# Build

FROM docker.io/library/fedora:32 as builder

RUN dnf -y install curl openssl-devel npm gcc make

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

ENV PATH "$PATH:/root/.cargo/bin"

RUN rustup target add wasm32-unknown-unknown

RUN cargo install wasm-pack
ENV WASM_PACK_PATH "/root/.cargo/bin/wasm-pack"

RUN mkdir -p /usr/src
VOLUME /usr/src/

ENV CARGO_HOME "/usr/src/.cargo-container-home"
