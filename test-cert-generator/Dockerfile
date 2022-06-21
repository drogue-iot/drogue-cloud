FROM ghcr.io/ctron/kubectl:1.21

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

VOLUME /etc/drogue-certs

RUN microdnf install -y make openssl

RUN mkdir -p /usr/src

ADD test-cert-generator/scripts/ /usr/src/

WORKDIR /usr/src

ENV \
    EGEN=/etc/drogue-certs

ENTRYPOINT [ "make" ]
