//! Build command descriptions and the shell boundary that executes Cargo.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CargoCommand {
    pub(crate) workspace: PathBuf,
    pub(crate) args: Vec<OsString>,
}

impl CargoCommand {
    pub(crate) fn test(workspace: &Path, target: &Path) -> Self {
        Self {
            workspace: workspace.to_owned(),
            args: vec![
                "test".into(),
                "--offline".into(),
                "--profile".into(),
                "mutants".into(),
                "--target-dir".into(),
                target.as_os_str().to_owned(),
            ],
        }
    }

    pub(crate) fn execute(&self) -> io::Result<ExitStatus> {
        Command::new("cargo")
            .current_dir(&self.workspace)
            .args(&self.args)
            .status()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    command: CargoCommand,
}

impl Prepared {
    pub(crate) fn new(command: CargoCommand) -> Self {
        Self { command }
    }

    pub(crate) fn execute(&self) -> io::Result<ExitStatus> {
        self.command.execute()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_command_selects_the_mutants_profile() {
        let command = CargoCommand::test(Path::new("subject"), Path::new("target-0"));
        let expected: Vec<OsString> = [
            "test",
            "--offline",
            "--profile",
            "mutants",
            "--target-dir",
            "target-0",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert_eq!(command.args, expected);
        assert_eq!(command.workspace, Path::new("subject"));
    }
}
