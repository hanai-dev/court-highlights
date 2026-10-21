# Development + CI image: the box's target OS (Ubuntu 26.04 LTS) with the pinned toolchain.
# Test-only tools (MediaMTX, FFmpeg) fake RTSP cameras; they are never part of the shipped box software.
FROM ubuntu:26.04

ARG RUST_VERSION=1.99.0
ARG MEDIAMTX_VERSION=v1.21.2
ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl ca-certificates git \
      libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
      gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
      libopenh264-8 intel-media-va-driver \
      ffmpeg \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION} \
    && rustup component add clippy rustfmt

RUN curl -fsSL -o /tmp/mediamtx.tar.gz \
      https://github.com/bluenviron/mediamtx/releases/download/${MEDIAMTX_VERSION}/mediamtx_${MEDIAMTX_VERSION}_linux_amd64.tar.gz \
    && tar -xzf /tmp/mediamtx.tar.gz -C /usr/local/bin mediamtx \
    && rm /tmp/mediamtx.tar.gz

WORKDIR /work
