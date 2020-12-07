# Build

FROM builder as builder

RUN unset CARGO_HOME && cargo install diesel_cli --no-default-features --features postgres

FROM registry.access.redhat.com/ubi8-minimal

LABEL org.opencontainers.image.source="https://github.com/drogue-iot/drogue-cloud"

RUN microdnf install -y libpq
COPY --from=builder /root/.cargo/bin/diesel /usr/local/bin/
RUN mkdir /migrations
COPY database-common/migrations /migrations

ENTRYPOINT ["/usr/local/bin/diesel"]

ENV RUST_LOG "diesel=debug"

CMD ["migration", "run"]
