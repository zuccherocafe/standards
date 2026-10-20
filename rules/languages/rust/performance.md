# Rust standard: Performance

Part of the [Rust standard](README.md). This file covers hot paths, zero-cost checks, data layout, parallelism, and measuring.

## Performance

### Hot paths

*Default.* A crate that runs per frame or per item records its hot path, budget, and measurement method in its crate documentation ([Know the hot paths and their budgets](../../general.md#performance)). Once warm, per-frame and per-item code in a crate on a real-time or engine path allocates nothing, and an offline crate weighs allocations against its named budget. To avoid allocating on each call, reuse buffers owned by the caller or held in a scratch structure that the caller retains. Test the zero-allocation promise in a dedicated integration-test binary with a counting global allocator from a small development dependency, so no other test shares the allocator and the test needs no `unsafe`. Size collections once from known information with `with_capacity` or `reserve` rather than growing them per item; a count read from a file is not known information until the bytes that back it have been read.

Ref: rust-analyzer's "Avoid Allocations", "Push Allocations to the Call Site" and "Avoid Intermediate Collections" (`docs/book/src/contributing/style.md`).

**Checked by:** the counting-allocator test; Clippy's `vec_init_then_push` for some incremental growth; review.

### Zero-cost abstractions, verified

*Weigh.* Verify rather than assume that newtypes, small `Copy` types, and checked constructors cost nothing in release builds: when a change adds types to a hot path, report its instruction count before and after. Make each scalar newtype a single-field `Copy` struct, and add `#[repr(transparent)]` only for a layout promise, such as FFI, `bytemuck`, or transmutes, never for speed. Rely on Rust's inlining of tiny functions for small cross-crate accessors and conversions, and add `#[inline]` when a profile or `nm` on the release binary shows a hot function left out of line, including that measurement in the commit message.

**Checked by:** rustc's `missing_copy_implementations` for public types that could be `Copy`; review of the reported measurements.

### Copies and data layout

*Weigh.* Pass large values by reference and small `Copy` values by value, and never clone on a hot path merely to satisfy the borrow checker. Store per-item data in contiguous vectors indexed by typed indices ([Index spaces](design.md#index-spaces)) rather than in maps or pointer graphs. In a core's inner loop, prefer `f64` arithmetic on plain values and convert to unit types at the function boundary, checking each value once ([Quantities](design.md#quantities)). Avoid generic functions instantiated across many types at crate boundaries, as rust-analyzer's "Avoid Monomorphization" advises, because monomorphised copies cost compile time and instruction cache.

**Checked by:** Clippy's pedantic `needless_pass_by_value`, `trivially_copy_pass_by_ref`, and `large_types_passed_by_value`; review.

### Parallelism

*Default.* Parallelise independent items in the largest groups that keep every core busy, using `std::thread::scope` and an atomic cursor without a runtime. Return results in item order regardless of thread execution order. Core results never depend on thread count. Read the thread count from `available_parallelism`, which honours CPU affinity so a pinned run measures the selected CPUs. In the crate documentation, state the group size and note that the slowest single item bounds wall time.

**Checked by:** review; the result-order promise by a test.

### Measuring

*Default.* Measure only release builds compiled with the workspace's own release profile. Under [Measure before claiming](../../general.md#performance), count instructions retired (`perf stat -e instructions:u`) to compare revisions, report a budget's CPU and wall time with the median, range, and run count, and report peak memory for builds. Profiles use frame pointers (`-C force-frame-pointers=yes`) and line tables in a separate target directory, leaving the measured binary unchanged. Benchmark harnesses are development dependencies of the measured crate, admitted like other test-only dependencies, and never run in the project's checks. Prefer divan, or criterion where the project already uses it.

Ref: jiff keeps its Criterion suite in a separate `bench/` crate; ripgrep's `benchsuite/` records runs per machine and date; rust-analyzer's `xtask metrics` records build and analysis times rather than failing on them.

**Checked by:** review of the reported measurements.
