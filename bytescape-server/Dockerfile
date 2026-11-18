# Build stage — compile the Rust backend
FROM rust:1.82 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

# Runtime stage — minimal Debian image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/bytescape-server /app/bytescape-server
COPY migrations /app/migrations
EXPOSE 8080
ENTRYPOINT ["/app/bytescape-server"]