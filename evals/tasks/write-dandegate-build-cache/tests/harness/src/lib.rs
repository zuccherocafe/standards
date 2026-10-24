//! Black-box cache checks against the executable named by `DANDEGATE_BIN`.

#[cfg(test)]
mod caches;
#[cfg(test)]
mod eviction;
#[cfg(test)]
#[allow(
    dead_code,
    reason = "support is copied from the upstream scenario harness"
)]
mod support;
