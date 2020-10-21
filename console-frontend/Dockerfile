FROM docker.io/library/fedora:32 as builder

RUN dnf -y install curl openssl-devel npm gcc

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

ENV PATH "$PATH:/root/.cargo/bin"

RUN rustup target add wasm32-unknown-unknown

RUN mkdir /src
COPY . /src

WORKDIR /src

RUN npm install
RUN npm run build

WORKDIR /

FROM registry.access.redhat.com/ubi8/ubi-minimal:latest

RUN microdnf -y install nginx

RUN mkdir /public
COPY --from=builder /src/dist/* /public/
COPY nginx.conf /etc/nginx/nginx.conf

CMD ["/usr/sbin/nginx", "-g", "daemon off;" ]

EXPOSE 8080
