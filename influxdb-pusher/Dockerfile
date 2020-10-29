FROM registry.access.redhat.com/ubi8-minimal

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

ADD target/release/influxdb-pusher /

ENTRYPOINT [ "/influxdb-pusher" ]
