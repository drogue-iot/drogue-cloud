FROM ghcr.io/drogue-iot/frontend-base:0.1.0

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

RUN mkdir /public

COPY console-frontend/nginx.conf /etc/nginx/nginx.conf

RUN mkdir /endpoints
VOLUME /endpoints
COPY console-frontend/nginx.sh /nginx.sh
RUN chmod a+x /nginx.sh
ENV BACKEND_URL "http://localhost:8011"

COPY console-frontend/dev/endpoints/ui-config.json /ui-config.template.json

CMD ["/nginx.sh"]

COPY console-frontend/dist/ /public/

EXPOSE 8080
