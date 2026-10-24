#!/usr/bin/env bash
set -euo pipefail
mkdir -p /app
cat > /app/findings.jsonl <<'FINDINGS'
{"file": "src/service.rs", "lines": [53, 58], "kind": "concurrency", "severity": "must-fix", "finding": "A grace timeout drops the old JoinHandle, detaching the old server task while its successor starts; abort and join it on expiry."}
{"file": "src/wasm.rs", "lines": [88, 96], "kind": "correctness", "severity": "must-fix", "finding": "ResultKey omits the peer's environment, so a second OFFSET returns the first OFFSET's cached output; key results on the environment too."}
{"file": "src/config.rs", "lines": [7, 8], "kind": "correctness", "severity": "must-fix", "finding": "RouteWeight deserialises without try_from, so a zero weight reaches a division by zero in select_weighted_peer; restore serde(try_from = \"u16\")."}
{"file": "src/service.rs", "lines": [117, 133], "kind": "correctness", "severity": "must-fix", "finding": "Content-Length is written from DOCUMENT.len() before the transform runs, so a failed transform's longer error body is framed as an eight-byte document; compute it from the output."}
{"file": "src/wasm.rs", "lines": [129, 131], "kind": "design", "severity": "should-fix", "finding": "input_digest is public only for private cache keys; make it private."}
FINDINGS
cat > /app/REVIEW.md <<'REVIEW'
# Live service reconfiguration review

Reviewed the full base-to-tip diff, the changed functions and their callers, `BRIEF.md`, the service design read and `evidence/checks.log`. The passing tests establish normal listener replacement, positive authored weights, repeated identical-input reuse and invocation-local environments for compiled code. They do not establish timeout ownership, environment-sensitive result reuse, constructor-equivalent deserialisation or the response framing when a transform fails. I have not rerun checks already evidenced or modified the checkout. The recorded immutable-module, sequential-admission and existing-adapter decisions remain in scope as stated.

## 1. Must-fix: a grace timeout detaches the old listener

In `src/service.rs`, `Service::reload` moves `old.handle` into `tokio::time::timeout`. A client can connect and send no bytes, then an operator reloads with a one-second grace budget and another listen address. The old `serve` remains inside its thirty-second read wait. When the grace expires the timeout drops the handle, which detaches rather than stops the old server task; the successor starts while the old task still owns its listener and connection. Reusing the same address can instead fail to bind. This breaks the brief's no-overlap and bounded-drain promises, not merely an imagined scheduling order.

The most specific rules are `/standards/rules/languages/rust/async.md#every-task-has-an-owner` and `#shut-down-in-dependency-order`; `#deadlines-propagate` explicitly distinguishes a timeout from task cancellation. Keep a mutable handle outside the timeout, await `timeout(grace, &mut handle)`, and on expiry call `abort()` and await the handle before starting its successor. Observe expected cancellation separately from task failure. This is a small lifecycle fix and needs a silent-client interruption regression, not a second supervisor.

## 2. Must-fix: the generated-result key loses the environment

In `src/wasm.rs`, `Transforms::apply` constructs `ResultKey` from only `wasm_path` and `input_digest`. Two positive-weight peers can name the same shift module with `OFFSET=1` and `OFFSET=2`; alternatively an operator changes that map at reload. After transforming `a` with the first configuration, the second returns cached `b` instead of `c`. With the service's fixed document, the corresponding shifted HTTP response is likewise stale output. The shared cache survives reload and bypasses `execute`, so a fresh execution store cannot correct the cache hit.

`/standards/rules/general.md#design`, **Generated output is typed and trusted**, permits reuse only while every input that shaped the result is unchanged. Add the canonical ordered environment map, or its stable digest, to `ResultKey` and test both configurations through `Transforms::apply`. This costs one key field and a regression; there is no need to rebuild modules when only the environment changes. The immutable-module deployment decision explains why live module-byte replacement is not an additional finding.

## 3. Must-fix: authored zero weights bypass the checked constructor

In `src/config.rs`, `RouteWeight` derives unchecked serde deserialisation even though `new(0)` refuses the value. An operator can provide a JSON configuration with one peer whose `weight` is `0`; `Config::from_json` accepts it and the first request reaches `ticket % total` with total zero in `select_weighted_peer`, causing a divide-by-zero panic in the listener task. The direct-constructor test and positive JSON fixture do not exercise this path.

`/standards/rules/languages/rust/design.md#invariants` requires preservation on every construction path, and the narrower `#deserialising` requires deserialisation to call the constructor. Restore `#[serde(try_from = "u16")]`; the existing `TryFrom<u16>` already delegates to `new`. This is a one-line fix plus a hand-written zero-weight JSON refusal test, with no repeated weight checks in the selector.

## 4. Must-fix: the header promises the document's length before the transform has run

In `src/service.rs`, `serve` now writes `Content-Length` from `DOCUMENT.len()` before calling `Transforms::apply`, on the stated ground that a byte transform preserves length. That holds only when the transform succeeds. When `apply` fails, for example because a module exhausts its fuel budget or its file cannot be read, the body is the error's message instead. The client is told the body is eight bytes long, reads the message's first eight bytes, such as `cannot r`, and treats them as the document; the rest arrives after the declared end. Before this change the header was computed from the bytes actually sent.

No narrower rule names response framing; this breaks the service's promise to serve a correct document under `/standards/rules/general.md#design`, **Correctness comes from the design first**. Compute the header from `output.len()` after the transform, as before, and decide the failure response separately rather than sending an error as the document. Taking the lock before writing the header costs nothing measurable for one eight-byte document. Add a regression with a peer whose module path does not exist and assert the declared length equals the body received.

## 5. Should-fix: keep the digest helper internal

`src/wasm.rs` exports `pub fn input_digest` solely for private cache-key construction. No other crate needs this helper. This exposes an implementation detail but does not change any response, so it is not a blocker. Under `/standards/rules/languages/rust/design.md#visibility`, make it private. The fix costs one visibility keyword and no API layer.

## Sound reuse and review limits

The compiled-module layer is sound: `CompiledComponentCache` depends on module bytes and engine settings, while the invocation's environment enters a fresh `Store` in `execute`. The test `fresh_stores_supply_environment_to_one_compiled_module` reuses the same module and observes both `b` and `c`. Environment belongs to the result identity, not to immutable compiled code. This follows `/standards/rules/general.md#design`, **Generated output is typed and trusted**, because compilation does not consume that input. No change to the compiled key is needed.

The existing design-read disagreement about reload completion remains justified until finding 1 is resolved; keep the row current then. The core byte ABI, sequential accepted-connection policy and adapter scheduling are recorded decisions, not new findings. No additional output format or removed-reader transition is introduced here.

**Verdict: fix first** — resolve findings 1–4 before landing; finding 5 is a small should-fix.

DONE: final
REVIEW
