FROM docker.io/library/ubuntu:24.04@sha256:dbdff34bb41cecdb07c79af373b44bb4c9ccba2520f014221fb95845f14bc6c1 AS toolchain
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gcc git jq libc6-dev python3 \
    && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -fsSLo /tmp/rustup-init https://static.rust-lang.org/rustup/archive/1.29.2/x86_64-unknown-linux-gnu/rustup-init \
    && echo "5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4  /tmp/rustup-init" | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain nightly-2026-09-24 -c rustfmt -c clippy \
    && rm /tmp/rustup-init

FROM toolchain AS source
RUN git clone -q https://github.com/zuccherocafe/standards.git /src \
    && git -C /src checkout -q d38fefcdc494673c9fe332856245a004487bc197
COPY start.patch /start.patch
RUN git -C /src apply --cached /start.patch \
    && test "$(git -C /src write-tree --prefix=tools/)" = 7c8becc2ae89bd35e3659865499bd3c445ad8e16 \
    && mkdir /export && git -C /src archive --prefix=tools/ "$(git -C /src write-tree --prefix=tools/)" | tar -x -C /export

FROM toolchain
COPY --from=source /export/ /app/
COPY AGENTS.md /app/AGENTS.md
COPY gitignore /app/.gitignore
WORKDIR /app
RUN cargo fetch --locked --manifest-path tools/Cargo.toml \
    && cargo test --locked --offline --manifest-path tools/Cargo.toml -p design-read --no-run \
    && git init -q -b main \
    && git add -A \
    && GIT_AUTHOR_DATE=2026-01-01T00:00:00Z GIT_COMMITTER_DATE=2026-01-01T00:00:00Z \
       git -c user.name=task -c user.email=task@localhost commit -q -m "The task's starting tree"
