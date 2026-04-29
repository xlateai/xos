FROM rust:1.88-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libasound2-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked --bin xos --bin xpy --bin xrs

FROM rust:1.88-bookworm AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    pkg-config \
    libasound2-dev \
    libasound2 \
    zip \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown
RUN cargo install wasm-pack --version 0.13.1 --locked
WORKDIR /app
COPY --from=builder /app/target/release/xos /usr/local/bin/xos
COPY --from=builder /app/target/release/xpy /usr/local/bin/xpy
COPY --from=builder /app/target/release/xrs /usr/local/bin/xrs
ENTRYPOINT ["xos"]
