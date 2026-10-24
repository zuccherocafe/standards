# Dandegate cache design read

- Area: Step execution, scratch checkouts and reusable build storage
- Last full read: 23695a7ebd2687f48c66a31a0fc14be860beffab
- Inspected: crates/dandegate/src/plan.rs, plan/template.rs, report.rs, report/identity.rs, report/schedule.rs, report/step.rs, report/step/supervise.rs, report/store.rs and revision/git.rs

## Models in play

`Plan` owns typed steps and recipes. `Template` separates authored placeholders from their expanded paths and `Expansion` supplies the paths for a particular instance. `Graph` schedules instances by step and place: before, after or both. `Session` owns report execution and expands recipes for commands and identity collectors.

`prepare_cache` makes a scratch directory owned by one report under `.cache/dandegate/scratch/`. The revision layer creates detached checkouts there, and `Session::remove_checkouts` removes them after execution. The adapters and transient command output also belong to the report. A supervisor manages each running command and reaps its descendants if the reporting process ends.

`Store` keeps complete successful step results under `.cache/dandegate/results/`, keyed by `Identity`. These are immutable outputs suitable for reuse, not writable compiler intermediates. Recipe identities keep authored placeholders symbolic instead of depending on transient output paths.

## Overlaps

A build command needs two lifetimes at once: a fresh source checkout for the compared revision and writable intermediates that survive the report. The report's scratch owner currently supplies only the first lifetime. Result storage supplies durable completed outputs but cannot safely act as a shared writable build directory.

## Half-done transitions

No persistent build-directory owner is wired into template expansion or report execution. Steps can put products in transient scratch storage or choose their own unmanaged shared location, but neither choice expresses cache ownership through command descendants.

## What later work needs

Cache-aware project recipes need a stable directory per step and place without making scratch paths part of result identities. Ownership must start before checkout, remain exclusive while any process can write, and permit overlapping reports to use private temporary caches. Eviction needs a last-use record for each whole instance directory and must share the same ownership boundary as writing.

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| Build storage lifetime | report execution | `crates/dandegate/src/report.rs` `prepare_cache`, `Session::remove_checkouts`; `crates/dandegate/src/plan/template.rs` `Expansion`; `crates/dandegate/src/report/step/supervise.rs` | Fresh scratch directories discard build intermediates, while unmanaged shared folders can be written by concurrent reports or descendants of a killed report. | A distinct persistent cache owner provides per-step and per-place folders, ownership inherited by writers, private contention fallback and bounded LRU eviction without changing scratch checkout or result identity lifetimes. |
