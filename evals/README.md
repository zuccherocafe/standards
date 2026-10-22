# Evals

## Tools

The development shell builds Harbor from [`pyproject.toml`](pyproject.toml) and `uv.lock` and provides jq. Before any run, `harbor --version` must print `0.24.0` and `jq --version` must print `jq-1.8.2`. The shell turns Harbor's usage telemetry off.

Harbor writes `~/.cache/harbor/notifications.json` on its first run, a path it takes from the home folder with no setting to move it.
