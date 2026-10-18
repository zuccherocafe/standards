# Rust standard: Async and concurrency

Part of the [Rust standard](README.md). Only for a project that runs an async runtime.

## Async and concurrency

A project that needs asynchronous I/O, such as a network service, follows this section. A project without that need forbids an async runtime rather than leaving it open. Each rule names its sources: Tokio 1.53 and tokio-util 0.7 for the runtime's contracts, and the source of the transport and storage crates a service uses for theirs.

### Keep computation synchronous

Cores take values and return values. They never await, import a runtime, read a clock, perform input or output, or spawn work ([Purity](design.md#purity)). The async shell supplies observations and time, calls the core's bounded synchronous transitions, and carries out the effects they request. A library's public interface offers domain values, and where it must, standard futures or a boundary trait, never a runtime handle or a runtime of its own. A synchronous function that takes substantial CPU time still does not belong on an executor thread. A synchronous core stays replayable and testable on values, and keeps the protocol's vocabulary free of a runtime.

Ref: Matthias Endler, [async Rust in practice](https://corrode.dev/blog/async/), on a synchronous core and an async shell.

**Checked by:** the dependency list and the project's dependency-edge check for runtime and network crates in cores; the report scans' purity scan for the file, process, environment, and clock paths it recognises, and their async scan for the runtime, network, and async I/O paths it names and for `async fn`, `async` blocks and closures, and `.await` in cores; review for what the scans miss, such as macro arguments and re-exports, and for spawning and indirect input, output, and blocking.

### Choose the scheduler in the binary

Use Tokio, the runtime the network ecosystem builds on. The binary chooses its runtime flavour explicitly and enables only the drivers it uses. A process with one owner of its state, mostly waiting on input and output, uses the current-thread scheduler. A process uses the multi-thread scheduler only when a measurement or a named dependency requires it, with its worker count set explicitly, since the default is one worker per CPU in every process. Use `LocalSet` or `LocalRuntime` only for tasks that must be `!Send`. Never create a runtime per request or inside a library. `tokio::spawn` requires `Send + 'static` on either flavour. `spawn_local` removes `Send`, not `'static`. Directly awaited and joined futures may borrow from their scope.

Ref: Tokio's [runtime](https://docs.rs/tokio/1.53.3/tokio/runtime/index.html), [`LocalSet`](https://docs.rs/tokio/1.53.3/tokio/task/struct.LocalSet.html), [`LocalRuntime`](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.LocalRuntime.html), and [`spawn`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn.html).

**Checked by:** the compiler for the bounds; a start-up test with the production runtime and features; review for the explicit choice and its measurement.

### One owner for state, short polls

Mutable domain state has one owner, and other tasks exchange bounded, typed messages with it. Never hold a synchronous lock guard or a `RefCell` borrow across an `await`. An async mutex guards a resource that genuinely needs asynchronous exclusive access, never state left inconsistent across an await. Blocking database calls, file work, and substantial computation run off the executor: `spawn_blocking` for a bounded job that finishes, a dedicated thread for a persistent blocking worker. Bound admission before spawning, including queued jobs and their input size. A started blocking job cannot be cancelled by the runtime, so it has cooperative checkpoints or runs in its own process. On a current-thread scheduler one blocking poll stalls every task.

Ref: Tokio's [`spawn_blocking`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html) and [`block_in_place`](https://docs.rs/tokio/1.53.3/tokio/task/fn.block_in_place.html).

**Checked by:** Clippy's `await_holding_lock` and `await_holding_refcell_ref` for the lock guards and `RefCell` borrows they recognise, and `await_holding_invalid_type` only for the types a project lists in `await-holding-invalid-types`; review for state ownership, other guards, blocking work, and admission bounds.

### Every task has an owner

Prefer composing with `await`, `join!`, or a bounded collection of futures in scope. Spawn only for an independent lifetime or scheduling need. Every spawned task belongs to a named supervisor, request, or session that defines its completion, cancellation, and join. A `JoinHandle` is never dropped to make work disappear: dropping it detaches the task rather than cancelling it, and loses its result, so its failures go unobserved. Use `JoinSet` when the owner needs each task's result and failure, and drain it while running. A `CancellationToken` signals intent; it neither owns nor waits for tasks. A third-party library's own driver tasks stop through that library's lifecycle API, documented where the library is wrapped.

Ref: Tokio's [`JoinHandle`](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinHandle.html) and [`JoinSet`](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinSet.html); tokio-util's [`CancellationToken`](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html).

**Checked by:** Clippy's `let_underscore_future` for `let _ =` on a future or handle, not for other ways a handle is dropped; tests that end with no task left running; review for each spawn's owner.

### Cancellation is specified at each async boundary

Each async operation documents what dropping its future can lose, what state stays valid, whether its effect may already have happened, and how its owner finishes cleanup. Every `select!`, timeout, fail-fast join, and abort is a cancellation point and is reviewed as one: the losing branches of a `select!` are dropped. Cooperative stop requests use hierarchical `CancellationToken`s, with a child token for each narrower lifetime. A cancelled token is not proof that anything stopped: actual completion, or an unknown outcome, is recorded separately.

Ref: Tokio's [`select!`](https://docs.rs/tokio/1.53.3/tokio/macro.select.html) on cancellation safety; tokio-util's [`CancellationToken`](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html); nextest's executor (`nextest-runner/src/runner/executor.rs`).

**Checked by:** review.

### Cancelling a wait does not undo an effect

Never cancel and restart a multi-step operation, such as `write_all` or `read_exact`, on a stream that is reused, without keeping its progress. Keep the operation's future alive, keep its progress in its owner, finish an accepted frame at a safe point, or close the stream and reconcile. Essential cleanup lives outside any future a timeout can drop. A side effect carries an action identity, an acknowledgement, and a generation check. If an effect may already have happened, a timeout or a lost acknowledgement leaves its outcome unknown, not failed, unless the operation's contract establishes otherwise. A timeout while waiting to submit, before any work was handed over, is known non-execution and is reported as an admission timeout or a refusal. Non-idempotent work is retried only after reconciliation, or under a deduplication contract that has been verified, so that one request never takes effect twice.

Ref: Tokio's [`mpsc::Sender::send`](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html) and `reserve` on what a cancelled send guarantees.

**Checked by:** tests that interrupt an operation at each await and reconcile; review.

### Bound work, not just channels

Every queue has a capacity, a maximum item size, an owner, and stated behaviour when full and when closed. Bound running tasks and waiting producers as well as buffered items: unbounded tasks waiting on a bounded channel are an unbounded queue. Reliable work waits for capacity only within its deadline, or is refused before it is accepted. A producer that may be cancelled holds its message while awaiting `reserve()` and sends through the permit. Updates that only the latest value matters for may be coalesced. Expired real-time data, such as audio or poses, is dropped, never replayed late. Stop and revocation travel on their own bounded or coalescing path, which obsolete work cannot bury.

Ref: Tokio's [`mpsc`](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/index.html) on bounded channels and backpressure.

**Checked by:** review of each queue's stated policy; tests of the full and closed cases.

### Deadlines propagate

Each external operation has one overall deadline on the monotonic clock, covering queueing, permits, and retries, and each layer takes its remaining budget from that deadline instead of restarting a full allowance. A streaming operation also has an idle timeout where it needs one. Results distinguish cancellation, timeout, overload, transport failure, and an unknown outcome. A Tokio timeout is not pre-emption: it cannot interrupt a blocking poll, stop a running blocking job, or prove the peer stopped. Timing out a spawned task's handle does not cancel the task, so its owner keeps the handle, requests cancellation, and joins or escalates.

Ref: Tokio's [`timeout`](https://docs.rs/tokio/1.53.3/tokio/time/fn.timeout.html).

**Checked by:** review; tests with paused time.

### Shut down in dependency order

Shutdown has explicit phases and a finite grace deadline: stop admitting work, request cancellation of what can be dropped, finish accepted obligations and required records, stop their dependencies, and join workers. The runtime, transport drivers, and record store stay alive while cleanup needs them. After the deadline, remaining tasks are aborted and their joins observed, and work that will not stop is escalated to the process's supervisor. Dropping the runtime or `shutdown_timeout` is not a graceful shutdown.

Ref: Tokio's guide to [graceful shutdown](https://tokio.rs/tokio/topics/shutdown).

**Checked by:** a shutdown test that ends with every owner's set empty and every required record written; review.

### Errors and panics at the owning boundary

A supervised task returns its operation's typed result, and its supervisor handles both that result and Tokio's `JoinError`, treating an expected abort, an unexpected cancellation, and a panic as different outcomes. The supervisor attaches the task's role and safe identifiers and renders the cause once ([Messages and codes](errors.md#messages-and-codes)). Panic payloads, request bodies, and credentials never reach a client. A panic in a spawned task ends only that task by default. A panic in a task that owns critical state starts a fail-closed shutdown and a bounded restart of the process, never continuation with possibly damaged state. A request that demonstrably shares no state may fail alone. The binary states its panic strategy, since `panic = "abort"` cannot be caught.

Ref: Tokio's [`JoinError`](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinError.html); the standard library's [`panic::set_hook`](https://doc.rust-lang.org/std/panic/fn.set_hook.html).

**Checked by:** tests of a panicking task at each owner; review.

### Test time and interruption deliberately

Test the synchronous core with supplied events, time, and inputs. Test the async shell with `#[tokio::test(start_paused = true)]`, bounded in-memory transports, and explicit handshakes at each interruption point, asserting observable outcomes and empty ownership sets rather than an executor's order or a sleep. Use seeded simulation where it exercises the real adapter's contract, and real transport and storage tests as well: paused time is not a simulated network, a crashed database, or a thread scheduler.

Ref: Tokio's [`pause`](https://docs.rs/tokio/1.53.3/tokio/time/fn.pause.html) and [`advance`](https://docs.rs/tokio/1.53.3/tokio/time/fn.advance.html); [Turmoil](https://docs.rs/turmoil/0.7.2/turmoil/) for seeded network simulation.

**Checked by:** review of the tests.
