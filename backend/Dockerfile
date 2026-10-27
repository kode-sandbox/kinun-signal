FROM rust:bookworm AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y gcc-x86-64-linux-gnu && \
    rustup target add x86_64-unknown-linux-gnu
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
RUN cargo build --release --target x86_64-unknown-linux-gnu

FROM --platform=linux/amd64 debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/stock_analyst_backend /usr/local/bin/
EXPOSE 8080
CMD ["stock_analyst_backend"]
