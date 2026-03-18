FROM rust:1.91-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release

FROM ubuntu:24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    openssh-client \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/gate /usr/local/bin/gate

ENTRYPOINT ["gate"]
