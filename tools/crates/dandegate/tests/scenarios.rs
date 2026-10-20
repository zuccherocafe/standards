//! Tests dandegate through its command, on small git repositories in the system's temporary folder. Each step is a
//! shell script in the repository's `dandegate/` folder, and a stand-in `nix` runs it as given. Steps that must
//! overlap or wait for each other wait on each other's events, never on time.
//!
//! dandegate's uses, most common first:
//!
//! 1. A report between two committed revisions, whose steps run on each revision and whose images, measures and
//!    sections dandegate compares on one page (`invocation`, `comparisons`).
//! 2. The same report again, which reuses the kept results and misses whenever what they depend on changes (`reuse`,
//!    `receipts`).
//! 3. Optional steps chosen with `--with`, needs and live steps (`selection`), run up to four at once with exclusive
//!    steps last (`scheduling`).
//! 4. A page that stands alone, and earlier reports read back (`durable`).
//! 5. A Rust workspace's built-in reports (`rust`).
//! 6. Host programs a step takes from the caller's machine (`host`).
//! 7. A step's build cache, kept between reports (`caches`), and a step's share of the CPUs (`cpus`).
//! 8. Every refusal, before any step runs (`invocation`).

#[path = "scenarios/caches.rs"]
mod caches;
#[path = "scenarios/comparisons.rs"]
mod comparisons;
#[path = "scenarios/cpus.rs"]
mod cpus;
#[path = "scenarios/durable.rs"]
mod durable;
#[path = "scenarios/host.rs"]
mod host;
#[path = "scenarios/invocation.rs"]
mod invocation;

#[path = "scenarios/receipts.rs"]
mod receipts;
#[path = "scenarios/reuse.rs"]
mod reuse;
#[path = "scenarios/rust.rs"]
mod rust;
#[path = "scenarios/scheduling.rs"]
mod scheduling;
#[path = "scenarios/selection.rs"]
mod selection;
#[path = "scenarios/support.rs"]
mod support;
