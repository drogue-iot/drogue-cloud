# Build

FROM ghcr.io/drogue-iot/diesel-base:0.2.0

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

RUN mkdir /migrations
COPY database-common/migrations /migrations

ENTRYPOINT ["/usr/local/bin/diesel"]

ENV RUST_LOG "diesel=debug"

CMD ["migration", "run"]
