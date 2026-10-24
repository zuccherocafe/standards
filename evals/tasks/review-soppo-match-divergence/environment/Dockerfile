FROM localhost/standards-eval-base:rust-pi-1.4.0

WORKDIR /app
COPY repo/ /app/
COPY change.patch /tmp/change.patch
RUN git init -b main && git config user.name 'Soppo maintainer' \
    && git config user.email 'maintainer@example.invalid' \
    && git add . && git commit -m 'infer: establish conservative statement flow' \
       -m 'Keep nullable access and missing-return diagnostics on a shared termination predicate. Leave matches conservative until enum coverage is available.' \
    && git rev-parse HEAD > /etc/eval-base-rev \
    && git apply /tmp/change.patch && rm /tmp/change.patch \
    && git add src \
    && git commit -m 'infer: recognise returning enum matches' \
       -m 'Reuse enum-match termination in missing-return checks and nil narrowing. Invert package loading through the core source-reader boundary so filesystem access stays in the CLI.' \
    && git add tests BRIEF.md docs evidence \
    && git commit -m 'infer: exercise match-flow inference and record evidence' \
       -m 'Cover returning enum matches and their nil-flow consumer, check guarded syntax, and generate integer return values for the enum termination property.' \
    && cargo fetch --locked \
    && cargo test --offline --locked --no-run

ENV CARGO_NET_OFFLINE=true
COPY condition_overlay/ /opt/condition_overlay/
RUN if [ -f /opt/condition_overlay/bin/design-read ]; then install /opt/condition_overlay/bin/* /usr/local/bin/; fi \
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
    && rm -rf /tmp/condition_overlay
