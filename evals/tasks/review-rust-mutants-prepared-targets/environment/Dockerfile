FROM localhost/standards-eval-base:rust-pi-1.4.0

WORKDIR /app
COPY repo/ /app/
COPY change.patch /tmp/change.patch
RUN git init -b main && git config user.name 'Tools maintainer' \
    && git config user.email 'maintainer@example.invalid' \
    && git add . && git commit -m 'rust-mutants: establish isolated mutation workers' \
       -m 'Run authored source replacements in private Cargo workspaces and target directories. Record the preparation-type disagreement before adding a warm path.' \
    && git rev-parse HEAD > /etc/eval-base-rev \
    && git apply /tmp/change.patch && rm /tmp/change.patch \
    && git add tools \
    && git commit -m 'rust-mutants: add prepared target prebuilding' \
       -m 'Accept a reusable target path, compile the subject before worker execution and run the planned commands against the selected target. Preserve cold targets when no seed is selected.' \
    && git add BRIEF.md docs evidence \
    && git commit -m 'rust-mutants: document prepared execution and check evidence' \
       -m 'Describe the completed preparation path and report command, parser and partitioning checks. Keep wall-time observations outside acceptance checks.' \
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
    && rm -rf /tmp/condition_overlay
