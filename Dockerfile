# Needs >= rustc 1.92 (Burn/CubeCL `cubek-*`); align with repo dev toolchain (~1.94)
FROM rust:1.94-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# bindgen (`v4l2-sys-mit`, etc.), `sentencepiece-sys` CMake build
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    clang \
    git \
    libclang-dev \
    pkg-config \
    libasound2-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
# Path crates from [patch]: must exist before chef cook (chef stage has no full tree yet)
COPY src/core/patches/ct2rs src/core/patches/ct2rs
COPY src/core/patches/gpu-allocator src/core/patches/gpu-allocator
COPY src/core/patches/sentencepiece-sys src/core/patches/sentencepiece-sys
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo install --path . --locked --root /usr/local --bin xos --bin xpy --bin xrs

FROM rust:1.94-bookworm AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    clang \
    cmake \
    git \
    libclang-dev \
    pkg-config \
    libasound2-dev \
    libasound2 \
    zip \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown
RUN cargo install wasm-pack --version 0.13.1 --locked
WORKDIR /app
COPY --from=builder /usr/local/bin/xos /usr/local/bin/xos
COPY --from=builder /usr/local/bin/xpy /usr/local/bin/xpy
COPY --from=builder /usr/local/bin/xrs /usr/local/bin/xrs
ENTRYPOINT ["xos"]
