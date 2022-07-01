FROM ghcr.io/drogue-iot/diesel-base:0.2.0

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

# install for postgres driver of diesel
RUN microdnf install -y libpq

ADD target/release/drogue-cloud-server /

RUN mkdir -p /usr/share/drogue-cloud/server/ui
ADD console-frontend/dist /usr/share/drogue-cloud/server/ui
RUN ls -laR /usr/share/drogue-cloud/server/ui
ENV UI_DIST=/usr/share/drogue-cloud/server/ui

ENTRYPOINT [ "/drogue-cloud-server" ]
