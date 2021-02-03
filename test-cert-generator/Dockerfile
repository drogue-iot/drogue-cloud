ARG builder="ghcr.io/drogue-iot/builder:0.1.5"

FROM ${builder}

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

VOLUME /etc/drogue-certs

RUN mkdir -p /usr/src

ADD test-cert-generator/scripts/ /usr/src/

WORKDIR /usr/src

ENV \
    EGEN=/etc/drogue-certs

ENTRYPOINT [ "make" ]
