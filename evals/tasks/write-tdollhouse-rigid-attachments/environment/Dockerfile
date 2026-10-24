FROM localhost/standards-eval-base:rust-pi-1.4.0

WORKDIR /app
COPY repo/ /app/
RUN cargo fetch --locked \
    && cargo test --locked --offline --workspace --no-run \
    && git init --initial-branch=main \
    && git config user.name 'Evaluation' \
    && git config user.email 'evaluation@example.invalid' \
    && git add . \
    && git commit -m 'convert: prepare attachment workspace' \
        -m 'Keep the cut mesh, skinning and spring pose together for offline conversion.' \
    && git rev-parse HEAD > /etc/eval-base-rev

ENV CARGO_NET_OFFLINE=true
COPY condition_overlay/ /tmp/condition_overlay/
RUN if [ -f /tmp/condition_overlay/bin/design-read ]; then install /tmp/condition_overlay/bin/* /usr/local/bin/; fi \
    && design-read check \
    && if [ -d /tmp/condition_overlay/standards ]; then \
        cp -a /tmp/condition_overlay/standards /standards; \
        if [ -f /tmp/condition_overlay/standards_agents_append.md ]; then \
            cat /tmp/condition_overlay/standards_agents_append.md >> /app/AGENTS.md; \
        fi; \
        mkdir -p /root/.agents/skills; \
        for skill in /standards/skills/*/; do ln -sfn "${skill%/}" /root/.agents/skills/; done; \
    elif [ -d /tmp/condition_overlay/skills ]; then \
        mkdir -p /opt/agent-skills /root/.agents/skills; \
        cp -a /tmp/condition_overlay/. /opt/agent-skills/; \
        if [ -f /opt/agent-skills/AGENTS.md ]; then \
            printf '\n' >> /app/AGENTS.md; \
            cat /opt/agent-skills/AGENTS.md >> /app/AGENTS.md; \
        fi; \
        for skill in /opt/agent-skills/skills/*/; do ln -sfn "${skill%/}" /root/.agents/skills/; done; \
    fi \
    && rm -rf /tmp/condition_overlay
