FROM registry.access.redhat.com/ubi8-minimal

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

VOLUME /etc/drogue-certs

RUN microdnf install -y make openssl

RUN curl -LO https://storage.googleapis.com/kubernetes-release/release/$(curl -s https://storage.googleapis.com/kubernetes-release/release/stable.txt)/bin/linux/amd64/kubectl
RUN chmod +x ./kubectl
RUN mv ./kubectl /usr/local/bin

RUN mkdir -p /usr/src

ADD test-cert-generator/scripts/ /usr/src/

WORKDIR /usr/src

ENV \
    EGEN=/etc/drogue-certs

ENTRYPOINT [ "make" ]
