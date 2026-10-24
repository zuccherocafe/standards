//! Assign mutation cases to workers, each owning its workspace and build directory.

use crate::command::{CargoCommand, Prepared};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;

#[derive(Debug)]
pub(crate) struct Shard {
    workspace: PathBuf,
    target: PathBuf,
    cases: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MutationResult {
    pub(crate) case: PathBuf,
    pub(crate) caught: bool,
}

pub(crate) fn plan(root: &Path, count: usize, cases: &[PathBuf]) -> Vec<Shard> {
    (0..count)
        .map(|index| Shard {
            workspace: root.join(format!("shard-{index}/workspace")),
            target: root.join(format!("shard-{index}/target")),
            cases: cases.iter().skip(index).step_by(count).cloned().collect(),
        })
        .collect()
}

fn stage_workspace(subject: &Path, workspace: &Path) -> io::Result<()> {
    fs::create_dir_all(workspace.join("src"))?;
    fs::copy(subject.join("Cargo.toml"), workspace.join("Cargo.toml"))?;
    fs::copy(subject.join("src/lib.rs"), workspace.join("src/lib.rs"))?;
    Ok(())
}

fn run_shard(subject: &Path, shard: &Shard) -> io::Result<Vec<MutationResult>> {
    stage_workspace(subject, &shard.workspace)?;
    let mut results = Vec::with_capacity(shard.cases.len());
    for case in &shard.cases {
        fs::copy(case, shard.workspace.join("src/lib.rs"))?;
        let outputs = shard.target.join("mutants");
        match fs::remove_dir_all(outputs) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let command = Prepared::new(CargoCommand::test(&shard.workspace, &shard.target));
        results.push(MutationResult {
            case: case.to_owned(),
            caught: !command.execute()?.success(),
        });
    }
    Ok(results)
}

pub(crate) fn run(subject: &Path, shards: &[Shard]) -> io::Result<Vec<MutationResult>> {
    thread::scope(|scope| {
        let handles: Vec<_> = shards
            .iter()
            .map(|shard| scope.spawn(move || run_shard(subject, shard)))
            .collect();
        let mut results = Vec::new();
        for handle in handles {
            let shard_results = handle
                .join()
                .map_err(|_| io::Error::other("shard panicked"))??;
            results.extend(shard_results);
        }
        results.sort_by(|left, right| left.case.cmp(&right.case));
        Ok(results)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_are_partitioned_once_in_authored_order() {
        let cases: Vec<PathBuf> = ["a.rs", "b.rs", "c.rs"].map(PathBuf::from).into();
        let shards = plan(Path::new("run"), 2, &cases);
        assert_eq!(
            shards[0].cases,
            [PathBuf::from("a.rs"), PathBuf::from("c.rs")]
        );
        assert_eq!(shards[1].cases, [PathBuf::from("b.rs")]);
        assert_eq!(shards[0].target, Path::new("run/shard-0/target"));
        assert_eq!(shards[1].target, Path::new("run/shard-1/target"));
    }
}
