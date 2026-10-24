FROM localhost/standards-eval-base:rust-pi-1.4.0

WORKDIR /app
COPY repo/ /app/
ENV CARGO_BUILD_JOBS=4
RUN cargo fetch --locked \
    && cargo build --locked --offline --workspace \
    && cargo test --locked --offline -p dandegate --no-run \
    && install /app/target/debug/rust-scans /usr/local/bin/
ENV CARGO_NET_OFFLINE=true

RUN git init --quiet --initial-branch=main \
    && git config user.name 'Evaluation Writer' \
    && git config user.email 'writer@example.invalid' \
    && git config commit.gpgsign false \
    && git add . \
    && git commit --quiet -m 'dandegate: prepare the isolated workspace' \
        -m 'Keep the source snapshot and design read together for offline development.' \
    && git rev-parse HEAD > /etc/eval-base-rev

COPY condition_overlay/ /tmp/condition_overlay/
RUN if [ -f /tmp/condition_overlay/bin/design-read ]; then install /tmp/condition_overlay/bin/* /usr/local/bin/; fi \
    && design-read check \
    && if [ -d /tmp/condition_overlay/standards ]; then \
        cp -R /tmp/condition_overlay/standards /standards; \
        if [ -f /tmp/condition_overlay/standards_agents_append.md ]; then \
            cat /tmp/condition_overlay/standards_agents_append.md >> /app/AGENTS.md; \
        fi; \
        mkdir -p /root/.agents/skills; \
        for skill in /standards/skills/*/; do ln -sfn "${skill%/}" /root/.agents/skills/; done; \
    elif [ -d /tmp/condition_overlay/skills ]; then \
        mkdir -p /opt/agent-skills /root/.agents/skills; \
        cp -R /tmp/condition_overlay/. /opt/agent-skills/; \
        if [ -f /opt/agent-skills/AGENTS.md ]; then \
            printf '\n' >> /app/AGENTS.md; \
            cat /opt/agent-skills/AGENTS.md >> /app/AGENTS.md; \
        fi; \
        for skill in /opt/agent-skills/skills/*/; do ln -sfn "${skill%/}" /root/.agents/skills/; done; \
    fi \
    && rm -rf /tmp/condition_overlay
