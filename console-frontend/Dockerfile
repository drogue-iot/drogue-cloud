FROM registry.access.redhat.com/ubi8/ubi-minimal:latest

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

RUN microdnf -y install nginx jq

RUN mkdir /public
COPY console-frontend/dist/ /public/
COPY console-frontend/nginx.conf /etc/nginx/nginx.conf

RUN mkdir /endpoints
VOLUME /endpoints
COPY console-frontend/nginx.sh /nginx.sh
RUN chmod a+x /nginx.sh
ENV BACKEND_URL "http://localhost:8011"

CMD ["/nginx.sh"]

EXPOSE 8080
