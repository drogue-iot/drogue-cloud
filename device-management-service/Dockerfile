FROM registry.access.redhat.com/ubi8-minimal

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

RUN microdnf install -y libpq
ADD target/release/drogue-cloud-device-management-service /

ENTRYPOINT [ "/drogue-cloud-device-management-service" ]
