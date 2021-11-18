FROM ghcr.io/drogue-iot/diesel-base:0.1.0

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

ADD target/release/drogue-cloud-server /

ENTRYPOINT [ "/drogue-cloud-server" ]
