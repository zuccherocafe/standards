# Base image for the standards evaluation tasks: Ubuntu 24.04 with the pinned nightly Rust toolchain and pi 1.4.0.
FROM docker.io/library/ubuntu:24.04

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gcc git jq libc6-dev python3 \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -fsSLo /tmp/rustup-init https://static.rust-lang.org/rustup/archive/1.29.2/x86_64-unknown-linux-gnu/rustup-init \
    && echo "5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4  /tmp/rustup-init" | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain nightly-2026-09-24 -c rustfmt -c clippy \
    && rm /tmp/rustup-init

COPY . /opt/pi/
RUN sh /opt/pi/install.sh

WORKDIR /app
CMD ["/bin/bash"]
