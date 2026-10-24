//! Exercise the byte bound through the CLI, using sparse files rather than allocating gigabytes of disk space.

use std::fs::{self, File};
use std::time::Duration;

use crate::support::Fixture;

#[test]
fn whole_caches_are_evicted_in_lru_order_to_the_byte_bound() {
    let fixture = Fixture::new("cache-lru");
    fixture.write(
        "dandegate.toml",
        r#"
[[steps.step]]
name = "trigger"
runs = "once"
run = ["true", "{cache}"]

[[steps.step]]
name = "oldest"
runs = "once"
optional = true
run = ["true", "{cache}"]

[[steps.step]]
name = "older"
runs = "once"
optional = true
run = ["true", "{cache}"]

[[steps.step]]
name = "newest"
runs = "once"
optional = true
run = ["true", "{cache}"]
"#,
    );
    let only = fixture.commit("only");
    let root = fixture.repo().join(".cache/dandegate/builds");
    fixture
        .run(
            "created",
            &["--before", &only, "--with", "oldest,older,newest"],
        )
        .page();
    for name in ["oldest", "older", "newest"] {
        fs::create_dir_all(root.join(name).join("both/deep")).expect("a cache subfolder");
    }
    for name in ["oldest", "older", "newest"] {
        fixture
            .run(
                &format!("used-{name}"),
                &["--before", &only, "--with", name, "--no-reuse"],
            )
            .page();
        // Separate real uses even when the filesystem records times only to whole seconds.
        std::thread::sleep(Duration::from_millis(1100));
    }
    let seed = |name: &str| {
        let cache = root.join(name).join("both");
        File::create(cache.join("deep/sparse"))
            .and_then(|file| file.set_len((16_u64 << 30) + 1))
            .expect("a sparse file");
        cache
    };
    let (oldest, older, newest) = (seed("oldest"), seed("older"), seed("newest"));
    let run = fixture.run("out", &["--before", &only]);
    run.page();
    assert!(
        !oldest.exists(),
        "the oldest whole folder exceeds the bound"
    );
    assert!(!older.exists(), "two removals are needed to reach 32 GiB");
    assert!(
        newest.join("deep/sparse").exists(),
        "the newest cache stays"
    );
    assert!(
        root.join("trigger/both").is_dir(),
        "the once step has a cache"
    );
    let log = run.file("report.log");
    let first = log
        .find("oldest/both")
        .expect("the log names the first eviction");
    let second = log
        .find("older/both")
        .expect("the log names the second eviction");
    assert!(
        first < second,
        "the log records least recently used first: {log}"
    );
    assert!(
        !log.contains("newest/both"),
        "the newest cache is not evicted: {log}"
    );
}
