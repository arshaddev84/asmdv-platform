FROM rust:1-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml ./Cargo.toml
COPY Cargo.lock ./Cargo.lock
COPY services ./services
RUN cargo build --locked --release -p inventory-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/inventory-service /usr/local/bin/inventory-service
EXPOSE 4101
CMD ["inventory-service"]