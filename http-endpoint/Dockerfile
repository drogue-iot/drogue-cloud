# Build 
FROM rust AS builder
WORKDIR /usr/src/
COPY common ./common

RUN USER=root cargo new http-endpoint 
WORKDIR /usr/src/http-endpoint
COPY ./http-endpoint/Cargo.toml .
RUN cargo build --release

COPY ./http-endpoint/src ./src
RUN cargo build --release 


# Bundle
FROM registry.access.redhat.com/ubi8-minimal

COPY --from=builder /usr/src/http-endpoint/target/release/http-endpoint /
USER 1000
CMD [ "/http-endpoint" ]
