FROM rust:1-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml ./Cargo.toml
COPY Cargo.lock ./Cargo.lock
COPY crates ./crates
COPY services ./services
RUN cargo build --locked --release -p procurement-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/procurement-service /usr/local/bin/procurement-service
EXPOSE 4102
CMD ["procurement-service"]