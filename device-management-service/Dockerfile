FROM registry.access.redhat.com/ubi8-minimal

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

RUN microdnf install -y libpq
ADD target/release/device-management-service /

ENTRYPOINT [ "/device-management-service" ]
