FROM localhost/standards-eval-base:rust-pi-1.4.0

WORKDIR /app
COPY repo/ /app/
COPY change.patch /tmp/change.patch
RUN git init -b main && git config user.name 'Makiatto maintainer' \
    && git config user.email 'maintainer@example.invalid' \
    && git add . && git commit -m 'service: establish the byte-transform listener' \
       -m 'Keep routing weights checked at configuration entry and scope the execution store to each invocation.' \
    && git rev-parse HEAD > /etc/eval-base-rev \
    && git apply /tmp/change.patch && rm /tmp/change.patch \
    && git add src Cargo.toml Cargo.lock \
    && git commit -m 'service: reload listeners and reuse transform work' \
       -m 'Share compiled modules and transformed responses across listener generations. Give reload a finite graceful-drain budget and accept configuration weights directly through serde.' \
    && git add BRIEF.md docs evidence \
    && git commit -m 'service: record reload acceptance evidence' \
       -m 'Document immutable module identities, invocation-local environment, and the scope of the passing regression checks.' \
    && cargo fetch --locked \
    && cargo test --offline --locked --no-run

ENV CARGO_NET_OFFLINE=true
COPY condition_overlay/ /opt/condition_overlay/
RUN if [ -f /opt/condition_overlay/bin/concord ]; then install /opt/condition_overlay/bin/* /usr/local/bin/; fi \
    && if [ -d /opt/condition_overlay/standards ]; then \
        cp -a /opt/condition_overlay/standards /standards; \
        if [ -f /opt/condition_overlay/standards_agents_append.md ]; then \
            cat /opt/condition_overlay/standards_agents_append.md >> /app/AGENTS.md; \
        fi; \
        mkdir -p /root/.agents/skills; \
        for skill in /standards/skills/*/; do ln -sfn "${skill%/}" /root/.agents/skills/; done; \
    elif [ -d /opt/condition_overlay/skills ]; then \
        mkdir -p /opt/agent-skills /root/.agents/skills; \
        cp -a /opt/condition_overlay/. /opt/agent-skills/; \
        if [ -f /opt/agent-skills/AGENTS.md ]; then \
            printf '\n' >> /app/AGENTS.md; \
            cat /opt/agent-skills/AGENTS.md >> /app/AGENTS.md; \
        fi; \
        for skill in /opt/agent-skills/skills/*/; do ln -sfn "${skill%/}" /root/.agents/skills/; done; \
    fi \
    && rm -rf /opt/condition_overlay
