# k2-falcon/Dockerfile  — distroless, ~12 MB
FROM --platform=linux/amd64 rust:1.75-slim AS builder
WORKDIR /build
COPY crates/falcon/Cargo.toml crates/falcon/Cargo.lock ./
COPY crates/falcon/src ./src
# ensure ca-certificates for TLS
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN cargo build --release --bin falcon

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /build/target/release/falcon /usr/local/bin/falcon
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV RUST_LOG=info
ENTRYPOINT ["/usr/local/bin/falcon"]
