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

# The change's base and the change, each checked against its tree id.
FROM toolchain AS source
RUN git clone -q https://github.com/zuccherocafe/standards.git /src \
    && git -C /src checkout -q d38fefcdc494673c9fe332856245a004487bc197
COPY tasks/review-design-read-new/environment/change.patch /change.patch
RUN test "$(git -C /src write-tree --prefix=tools/)" = 0b5e7bec86483fb58d14b6c0c93e3b5f94297156 \
    && mkdir -p /export/base /export/change \
    && git -C /src archive --prefix=tools/ "$(git -C /src write-tree --prefix=tools/)" | tar -x -C /export/base \
    && git -C /src apply --cached /change.patch \
    && test "$(git -C /src write-tree --prefix=tools/)" = 7c8becc2ae89bd35e3659865499bd3c445ad8e16 \
    && git -C /src archive --prefix=tools/ "$(git -C /src write-tree --prefix=tools/)" | tar -x -C /export/change

FROM toolchain
COPY pi/ /opt/pi/
RUN sh /opt/pi/install.sh
COPY findings/score.sh /opt/findings/score.sh
COPY --from=source /export/base/ /app/
COPY tasks/review-design-read-new/environment/AGENTS.md /app/AGENTS.md
COPY tasks/review-design-read-new/environment/gitignore /app/.gitignore
COPY --from=source /export/change/ /var/tmp/change/
COPY tasks/review-design-read-new/environment/message.txt /var/tmp/message.txt
WORKDIR /app
# Two commits with fixed dates: the base, then the change with its description.
RUN git init -q -b main \
    && git add -A \
    && GIT_AUTHOR_DATE=2026-01-01T00:00:00Z GIT_COMMITTER_DATE=2026-01-01T00:00:00Z \
       git -c user.name=task -c user.email=task@localhost commit -q -m "The change's base" \
    && rm -rf tools && mv /var/tmp/change/tools tools && rmdir /var/tmp/change \
    && git add -A \
    && GIT_AUTHOR_DATE=2026-01-02T00:00:00Z GIT_COMMITTER_DATE=2026-01-02T00:00:00Z \
       git -c user.name=task -c user.email=task@localhost commit -q -F /var/tmp/message.txt \
    && rm /var/tmp/message.txt \
    && test "$(git rev-parse HEAD:tools)" = 7c8becc2ae89bd35e3659865499bd3c445ad8e16 \
    && cargo fetch --locked --manifest-path tools/Cargo.toml \
    && cargo test --locked --offline --manifest-path tools/Cargo.toml -p design-read --no-run
