FROM docker.io/library/fedora:32 as builder

RUN dnf -y install curl openssl-devel npm gcc

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

ENV PATH "$PATH:/root/.cargo/bin"

RUN rustup target add wasm32-unknown-unknown

RUN mkdir /src
COPY . /src

WORKDIR /src/console-frontend

RUN npm install
RUN npm run build

WORKDIR /

FROM registry.access.redhat.com/ubi8/ubi-minimal:latest

RUN microdnf -y install nginx jq

RUN mkdir /public
COPY --from=builder /src/console-frontend/dist/ /public/
COPY console-frontend/nginx.conf /etc/nginx/nginx.conf

RUN mkdir /endpoints
VOLUME /endpoints
COPY console-frontend/nginx.sh /nginx.sh
RUN chmod a+x /nginx.sh
ENV BACKEND_URL "http://localhost:8011"

CMD ["/nginx.sh"]

EXPOSE 8080
