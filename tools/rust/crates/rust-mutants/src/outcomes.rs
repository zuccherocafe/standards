//! The shards' outcome lists, merged: cargo-mutants writes one file per outcome in each shard's `mutants.out`, one
//! mutant a line.

use std::fs;
use std::io;
use std::ops::Index;
use std::path::PathBuf;

/// What happened to a mutant, as cargo-mutants names its list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Caught,
    Missed,
    Timeout,
    Unviable,
}

impl Outcome {
    /// Every outcome, in the order the report counts them.
    pub(crate) const ALL: [Self; 4] = [Self::Caught, Self::Missed, Self::Timeout, Self::Unviable];

    /// The list's name, which is also its file's name without `.txt`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Caught => "caught",
            Self::Missed => "missed",
            Self::Timeout => "timeout",
            Self::Unviable => "unviable",
        }
    }

    fn slot(self) -> usize {
        match self {
            Self::Caught => 0,
            Self::Missed => 1,
            Self::Timeout => 2,
            Self::Unviable => 3,
        }
    }
}

/// Each outcome's mutants: each shard's list in shard order, each file's lines in order, duplicates kept, and a list a
/// shard did not write empty.
#[derive(Debug, Default)]
pub(crate) struct Outcomes([Vec<String>; 4]);

impl Outcomes {
    /// The lists the shards with these output folders wrote, in the order given.
    ///
    /// # Errors
    /// [`ReadError`] naming a list that exists but cannot be read as text.
    pub(crate) fn read(outputs: impl IntoIterator<Item = PathBuf>) -> Result<Self, ReadError> {
        let mut outcomes = Self::default();
        for output in outputs {
            for outcome in Outcome::ALL {
                let path = output
                    .join("mutants.out")
                    .join(format!("{}.txt", outcome.name()));
                if !path.exists() {
                    continue;
                }
                let text = fs::read_to_string(&path).map_err(|source| ReadError {
                    path: path.clone(),
                    source,
                })?;
                outcomes.0[outcome.slot()].extend(text.lines().map(str::to_owned));
            }
        }
        Ok(outcomes)
    }

    /// Whether every list is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.iter().all(Vec::is_empty)
    }
}

impl Index<Outcome> for Outcomes {
    type Output = [String];

    fn index(&self, outcome: Outcome) -> &[String] {
        &self.0[outcome.slot()]
    }
}

/// An outcome list that could not be read.
#[derive(Debug, thiserror::Error)]
#[error("could not read {}", .path.display())]
pub(crate) struct ReadError {
    path: PathBuf,
    #[source]
    source: io::Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shards_lists_merge_in_order_and_a_missing_list_is_empty() {
        let folder = crate::scratch("outcomes");
        let shards = [folder.join("shard-0"), folder.join("shard-1")];
        for (shard, mutant) in shards.iter().zip(["with 0", "with 1"]) {
            fs::create_dir_all(shard.join("mutants.out")).expect("writable");
            fs::write(
                shard.join("mutants.out/missed.txt"),
                format!("crates/core/src/lib.rs:1:22: replace one -> u8 {mutant}\n"),
            )
            .expect("writable");
        }
        fs::write(shards[0].join("mutants.out/caught.txt"), "").expect("writable");
        let outcomes = Outcomes::read(shards.clone());
        let _ = fs::remove_dir_all(&folder);
        let outcomes = outcomes.expect("readable");
        assert_eq!(
            outcomes[Outcome::Missed],
            [
                "crates/core/src/lib.rs:1:22: replace one -> u8 with 0",
                "crates/core/src/lib.rs:1:22: replace one -> u8 with 1",
            ]
        );
        assert_eq!(outcomes[Outcome::Caught], [] as [String; 0]);
        assert_eq!(outcomes[Outcome::Timeout], [] as [String; 0]);
        assert!(!outcomes.is_empty());
    }
}
