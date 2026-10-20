//! The build caches that outlive a report. `{cache}` names one folder for each step and place under the repository's
//! ignored `.cache/dandegate/builds/<step>/<place>/`, kept from one report to the next, so a step that builds into it
//! finds what its last run built.
//!
//! One report at a time owns all of them. A report whose steps name `{cache}` takes an `flock` on
//! `.cache/dandegate/builds.lock` before it checks out either revision, so its checkouts' files are always newer than
//! anything in the caches, and Cargo, for one, builds the workspace's own crates again and reuses only their
//! dependencies. Every process the report starts inherits the lock, so it is let go only when the last of them has
//! ended, even when dandegate itself is killed first. When another report holds the lock, each step gets a new folder
//! in this report's scratch folder instead, which goes with the scratch folder, and the page says so.
//!
//! The caches together are bounded by [`BOUND`]. At the end of its report, still holding the lock, dandegate removes
//! whole folders, least recently used first, until the total is under the bound, and its log names each one. Deleting
//! `.cache/dandegate/builds/` by hand while no report runs clears them all.

use std::fs::{self, File};
use std::io;
use std::os::fd::OwnedFd;
use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};
use rustix::fs::{FlockOperation, Mode, OFlags, flock, open};

use crate::plan::StepName;
use crate::report::schedule::Place;

/// The size the caches may reach together before the least recently used are removed: room for both revisions' release
/// and test builds of a large workspace.
pub(crate) const BOUND: u64 = 32 << 30;

/// The build caches, owned by this report until it is dropped and every process it started has ended.
#[derive(Debug)]
pub(crate) struct Builds {
    folder: Utf8PathBuf,
    _lock: OwnedFd,
}

impl Builds {
    /// The caches in `folder`, locked through the file `lock` outside it; none when another report holds them.
    ///
    /// # Errors
    /// Why the lock file cannot be opened or locked.
    pub(crate) fn take(folder: Utf8PathBuf, lock: &Utf8Path) -> io::Result<Option<Self>> {
        // Opened without close-on-exec, unlike every file the standard library opens, so each supervisor and command
        // inherits the lock, and it is let go only when the last process holding it ends: a step still writing after
        // dandegate is killed keeps the caches from the next report.
        let file = open(
            lock.as_std_path(),
            OFlags::CREATE | OFlags::RDWR,
            Mode::from_raw_mode(0o644),
        )?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Some(Self {
                folder,
                _lock: file,
            })),
            Err(rustix::io::Errno::WOULDBLOCK) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// The cache of `step` at `place`, made if missing and marked as used now, which the bound removes the oldest by.
    ///
    /// # Errors
    /// Why the folder cannot be made or marked.
    pub(crate) fn folder(&self, step: &StepName, place: Place) -> io::Result<Utf8PathBuf> {
        let folder = self.folder.join(step.as_str()).join(place.as_str());
        fs::create_dir_all(&folder)?;
        File::open(&folder)?.set_modified(SystemTime::now())?;
        Ok(folder)
    }

    /// Remove whole cache folders, least recently used first, until the caches together are no larger than `bound`.
    /// Gives a line for each folder removed, or that could not be.
    pub(crate) fn bound(&self, bound: u64) -> Vec<String> {
        let mut found: Vec<(SystemTime, Utf8PathBuf, u64)> = self
            .folders()
            .into_iter()
            .map(|folder| {
                let used = folder
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                let size = size_of(&folder);
                (used, folder, size)
            })
            .collect();
        found.sort();
        let mut total = found
            .iter()
            .fold(0_u64, |total, (_, _, size)| total.saturating_add(*size));
        let mut lines = Vec::new();
        for (_, folder, size) in found {
            if total <= bound {
                break;
            }
            match fs::remove_dir_all(&folder) {
                Ok(()) => {
                    total = total.saturating_sub(size);
                    lines.push(format!(
                        "removed the build cache {folder} ({size} bytes), the least recently used"
                    ));
                }
                Err(error) => lines.push(format!(
                    "the build cache {folder} could not be removed: {error}"
                )),
            }
        }
        lines
    }

    /// Every cache folder, `<step>/<place>`.
    fn folders(&self) -> Vec<Utf8PathBuf> {
        let folders_in = |folder: &Utf8Path| -> Vec<Utf8PathBuf> {
            folder.read_dir_utf8().map_or_default(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                    .map(|entry| entry.path().to_owned())
                    .collect()
            })
        };
        folders_in(&self.folder)
            .iter()
            .flat_map(|step| folders_in(step))
            .collect()
    }
}

/// The length of every file under `folder`, links not followed.
fn size_of(folder: &Utf8Path) -> u64 {
    let mut total = 0_u64;
    let mut pending = vec![folder.to_owned()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = current.read_dir_utf8() else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            // A directory entry's metadata does not follow a link.
            match entry.metadata() {
                Ok(metadata) if metadata.is_dir() => pending.push(entry.path().to_owned()),
                Ok(metadata) => total = total.saturating_add(metadata.len()),
                Err(_) => {}
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::revision::Side;

    /// A new folder for one test.
    fn root(name: &str) -> Utf8PathBuf {
        let temporary =
            Utf8PathBuf::try_from(std::env::temp_dir()).expect("a UTF-8 temporary folder");
        super::super::acquire(&temporary, name).expect("a folder of its own")
    }

    #[test]
    fn one_report_at_a_time_holds_the_caches() {
        let root = root("dandegate-cache-take-");
        let take = || Builds::take(root.join("builds"), &root.join("builds.lock")).expect("a lock");
        let first = take().expect("free");
        assert!(take().is_none(), "held by the first");
        let step = StepName::new("prepare").expect("a name");
        let folder = first
            .folder(&step, Place::Revision(Side::Before))
            .expect("a cache");
        assert_eq!(folder, root.join("builds/prepare/before"));
        assert!(folder.is_dir());
        drop(first);
        assert!(take().is_some(), "free again");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_least_recently_used_caches_are_removed_until_under_the_bound() {
        let root = root("dandegate-cache-bound-");
        let builds = Builds::take(root.join("builds"), &root.join("builds.lock"))
            .expect("a lock")
            .expect("free");
        let cache = |step: &str, age: u64| {
            let folder = root.join("builds").join(step).join("after");
            fs::create_dir_all(folder.join("deep")).expect("a folder");
            fs::write(folder.join("deep/file"), vec![0_u8; 300]).expect("a file");
            let used = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 - age);
            File::open(&folder)
                .and_then(|handle| handle.set_modified(used))
                .expect("a time");
            folder
        };
        let (oldest, older, newest) = (cache("a", 20), cache("b", 10), cache("c", 0));
        // 900 bytes: the two oldest go, leaving 300.
        assert_eq!(builds.bound(400).len(), 2);
        assert!(!oldest.exists() && !older.exists() && newest.exists());
        assert_eq!(builds.bound(400), Vec::<String>::new());
        let _ = fs::remove_dir_all(&root);
    }
}
