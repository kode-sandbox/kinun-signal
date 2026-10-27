FROM rust:bookworm AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y libpq-dev pkg-config libssl-dev && \
    cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/stock_analyst_backend /usr/local/bin/
EXPOSE 8080
CMD ["stock_analyst_backend"]
