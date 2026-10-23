# The Codex reviewer's image: Python for Harbor's check of the analysis, and Codex's own release package, checked
# against its SHA-256, so a review's setup installs nothing over the evaluator's locked network. The package keeps
# the helpers Codex runs from beside its binary, such as the code-mode host its tools need.
FROM docker.io/library/python:3.13-slim@sha256:d879ed88565a7a3c614b980b369d4a5025a9d057b71f93d09fc6d0b2ca823a18
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSLo /tmp/codex.tar.gz \
       https://github.com/openai/codex/releases/download/rust-v0.163.0/codex-package-x86_64-unknown-linux-musl.tar.gz \
    && echo "582646c6bf0561eb0de540f7fc85b9121b0091a826bc6e96830b7f1722122182  /tmp/codex.tar.gz" | sha256sum -c - \
    && mkdir /opt/codex \
    && tar -xzf /tmp/codex.tar.gz -C /opt/codex --no-same-owner

FROM docker.io/library/python:3.13-slim@sha256:d879ed88565a7a3c614b980b369d4a5025a9d057b71f93d09fc6d0b2ca823a18
COPY --from=0 /opt/codex /opt/codex
RUN ln -s /opt/codex/bin/codex /usr/local/bin/codex
# Harbor runs every command in the task's workdir, which Podman, unlike Docker, does not create.
WORKDIR /app
