# Dandegate cache design read

- Area: Step execution, scratch checkouts and reusable build storage
- Last full read: 23695a7ebd2687f48c66a31a0fc14be860beffab
- Inspected: crates/dandegate/src/plan.rs, plan/template.rs, report.rs, report/cache.rs, report/identity.rs, report/schedule.rs, report/step.rs, report/step/supervise.rs, report/store.rs and revision/git.rs

## Models in play

`Plan` owns typed steps and recipes. `Template` separates authored placeholders from expanded paths and recognises `{cache}` in command and collector arguments and environment values. `Step::uses_cache` derives cache use from those templates. `Graph` schedules instances by step and place: before, after or both. `Session` supplies each cache-using instance's directory to both command and collector expansion.

`prepare_cache` makes a scratch directory owned by one report under `.cache/dandegate/scratch/`. Detached checkouts, adapters, temporary output and contention fallback caches live there. The supervisor manages each running command and reaps its descendants if the reporting process ends. `Session::remove_checkouts` removes the report's transient checkout state.

`Builds` owns the persistent writable directories under `.cache/dandegate/builds/<step>/<place>/`. Its non-blocking file lock is outside that directory, acquired before checkout and inherited across process execution. Ownership remains with descendants after the reporter is killed. An unavailable lock sends each cache-using instance to private scratch storage and adds a notice to its report page. A report with no enabled cache-using step does not take this owner.

`Store` keeps immutable successful results under `.cache/dandegate/results/`, keyed by `Identity`. Recipe identities preserve the symbolic `{cache}` placeholder rather than a particular expanded directory. These results and writable build intermediates have separate owners and lifetimes.

## Overlaps

The report supplies both fresh revision checkouts and stable build directories to recipes without giving them the same lifetime. `Builds` supplies writable storage, while `Store` alone owns completed reusable outputs. Template-derived use and graph places determine which cache directories are supplied without a second list of cache-aware steps.

## Half-done transitions

There is one owner for persistent build storage and one explicit fallback for reports that cannot obtain it. Both feed the same expansion path; scratch checkout cleanup does not remove persistent products.

## What later work needs

Further cache-aware recipes can use `{cache}` for each instance without managing locks themselves. `Builds::folder` records last use on the whole instance directory; at report completion `Builds::bound` measures file lengths recursively without following links and removes least-recently-used whole directories until the total is at most 32 GiB. Eviction and writers share the same ownership boundary, and each attempted removal is recorded in the report log.

## Disagreements

None found.
