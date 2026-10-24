# Persistent build caches for dandegate

Extend `dandegate` in `/app` (`crates/dandegate`) so project steps can reuse persistent build directories across comparison reports without sharing writable state between concurrent reports.

## Requirements

- Support the `{cache}` placeholder in step commands, collectors, and environment values (alongside existing placeholders, keeping `{cache}` symbolic in result identities), resolving to `.cache/dandegate/builds/<step>/<place>/` (`before`, `after`, or `both`) for enabled steps that name `{cache}`.
- Acquire exclusive non-blocking ownership via `.cache/dandegate/builds.lock` before checkout and hold it until all processes started by the report have exited; when another report holds the lock, fall back to a private scratch directory and note the contention on the HTML report page.
- Leave caches and the lock untouched when no enabled step names `{cache}`, and bound persistent caches at 32 GiB (34,359,738,368 bytes) by evicting whole `<step>/<place>` directories in least-recently-used order (logging evictions to `report.log`).
- This is an unattended run with no interactive user replies; use your engineering judgement for any unstated implementation details and implement, test, and commit your complete change on the working branch in `/app`.
