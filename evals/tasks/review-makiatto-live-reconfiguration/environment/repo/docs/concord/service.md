# Service design read

- Area: listener task ownership, routing configuration and byte-filter execution.
- Last full read: the base snapshot recorded in `/etc/eval-base-rev`.
- Inspected: `src/service.rs`, `src/config.rs`, `src/wasm.rs` and their adjacent unit tests.

## Models in play

`Service` supervises the listener's stop sender and join handle. `shutdown` requests graceful completion, then awaits the task and propagates its result. The listener owns the accepted connection until the response completes or its read deadline expires.

`RouteWeight` privately stores a positive integer. Its constructor and serde conversion share the same check. Weighted selection relies on that invariant rather than checking weights again.

`Transforms` compiles module bytes and creates a fresh execution store for each invocation. The host offset comes from that invocation's environment, not the compiled code.

## Overlaps

None in this area.

## Half-done transitions

None in this area.

## What later work needs

A live reload needs a finite drain budget and an explicit owner until every old task is joined. Transform caching needs separate identities for compiled code and generated output.

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| Reload ownership | Service | `src/service.rs`, listener task | There is no finite reload operation | A reload waits for or stops and joins the old task before starting its successor |
