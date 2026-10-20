# Rust standard: Performance

Part of the [Rust standard](README.md). Hot paths, zero-cost checks, data layout, parallelism and measuring.

## Performance

### Hot paths

A crate that runs per frame or per item records its hot path, budget, and measurement method in its crate documentation ([Know the hot paths and their budgets](../../general.md#performance)). Once warm, per-frame and per-item code allocates nothing. Reuse buffers owned by the caller or by a scratch structure the caller retains. Test this promise with a counting global allocator in a dedicated integration-test binary, so no other test uses the allocator. Obtain the allocator from a small development dependency, so the test needs no `unsafe`. Size collections once from known information with `with_capacity` or `reserve`, rather than growing them per item. A count read from a file is not known information until the bytes that back it have been read.

Ref: rust-analyzer's "Avoid Allocations", "Push Allocations to the Call Site" and "Avoid Intermediate Collections" (`docs/book/src/contributing/style.md`).

**Checked by:** the counting-allocator test; Clippy's `vec_init_then_push` for some incremental growth; review.

### Zero-cost abstractions, verified

Check rather than assume that newtypes, small `Copy` types, and checked constructors cost nothing in release builds. A scalar newtype is a single-field `Copy` struct. Add `#[repr(transparent)]` only for a layout promise, such as FFI, `bytemuck`, or transmutes, never for speed. Small cross-crate accessors and conversions rely on Rust's inlining of tiny functions. Add `#[inline]` when a profile or `nm` on the release binary shows a hot function left out of line. Include the measurement in the commit message. A change that adds types to a hot path reports its instruction count before and after.

**Checked by:** rustc's `missing_copy_implementations` for public types that could be `Copy`; review of the reported measurements.

### Copies and data layout

Pass large values by reference and small `Copy` values by value. Never clone merely to satisfy the borrow checker in a hot path. Use contiguous per-item vectors with typed indices ([Index spaces](design.md#index-spaces)), rather than maps or pointer graphs. In a core's inner loop, prefer `f64` arithmetic on plain values. Convert to unit types at the function boundary, checking each value once ([Quantities](design.md#quantities)). Avoid generic code over many types at crate boundaries, as rust-analyzer's "Avoid Monomorphization" advises. Monomorphised copies cost compile time and instruction cache.

**Checked by:** Clippy's pedantic `needless_pass_by_value`, `trivially_copy_pass_by_ref`, and `large_types_passed_by_value`; review.

### Parallelism

Parallelise independent items in the largest groups that keep every core busy. Return results in item order, regardless of thread execution order. Use `std::thread::scope` and an atomic cursor, without a runtime. Document the group size and the wall-time bound, which is the largest item, in the crate documentation. Core results never depend on thread count. Obtain the thread count from `available_parallelism`. It honours CPU affinity, so a pinned run measures the selected CPUs.

**Checked by:** review; the result-order promise by a test.

### Measuring

Measure only release builds with the workspace's own profile. Under [Measure before claiming](../../general.md#performance), count instructions retired with `perf stat -e instructions:u` to compare revisions, and report a budget's CPU and wall time with the median, range, and run count. For builds, report peak memory. Profiles use frame pointers (`-C force-frame-pointers=yes`) and line tables in a separate target directory, leaving the measured binary unchanged. Benchmark harnesses are development dependencies of the measured crate, admitted like other test-only dependencies. Prefer divan, or criterion where the project already uses it. Benchmark harnesses are never part of the project's checks.

Ref: jiff keeps its Criterion suite in a separate `bench/` crate; ripgrep's `benchsuite/` records runs per machine and date; rust-analyzer's `xtask metrics` records build and analysis times rather than failing on them.

**Checked by:** review of the reported measurements.
