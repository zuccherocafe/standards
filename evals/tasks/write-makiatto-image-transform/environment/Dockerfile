FROM localhost/standards-eval-base:rust-pi-1.4.0

WORKDIR /app
COPY repo/ /app/
RUN cargo fetch --locked \
    && cargo build --offline --locked --all-targets \
    && cargo clippy --offline --locked --all-targets -- -D warnings \
    && git init -b main \
    && git config user.name Evaluation \
    && git config user.email evaluation@example.invalid \
    && git add . \
    && git commit -m 'web: extract domain-scoped static serving' \
        -m 'Keep the synchronous configuration and serving boundary isolated from transport, databases and network access.' \
    && git rev-parse HEAD > /etc/eval-base-rev

ENV CARGO_NET_OFFLINE=true
COPY condition_overlay/ /opt/condition_overlay/
RUN if [ -f /opt/condition_overlay/bin/concord ]; then install /opt/condition_overlay/bin/* /usr/local/bin/; fi \
    && concord check \
    && if [ -d /opt/condition_overlay/standards ]; then \
        cp -R /opt/condition_overlay/standards /standards; \
        if [ -f /opt/condition_overlay/standards_agents_append.md ]; then \
            cat /opt/condition_overlay/standards_agents_append.md >> /app/AGENTS.md; \
        fi; \
        mkdir -p /root/.agents/skills; \
        for skill in /standards/skills/*/; do ln -sfn "${skill%/}" /root/.agents/skills/; done; \
    elif [ -d /opt/condition_overlay/skills ]; then \
        mkdir -p /opt/agent-skills /root/.agents/skills; \
        cp -R /opt/condition_overlay/. /opt/agent-skills/; \
        if [ -f /opt/agent-skills/AGENTS.md ]; then \
            printf '\n' >> /app/AGENTS.md; \
            cat /opt/agent-skills/AGENTS.md >> /app/AGENTS.md; \
        fi; \
        for skill in /opt/agent-skills/skills/*/; do ln -sfn "${skill%/}" /root/.agents/skills/; done; \
    fi \
    && rm -rf /opt/condition_overlay
