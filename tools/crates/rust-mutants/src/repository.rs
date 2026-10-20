//! The Git repository around the workspace: its root, the change since a revision, its files and their copies.
//!
//! cargo-mutants copies only the workspace folder, so a crate that embeds a file from elsewhere in the repository does
//! not build in its copy. Each shard therefore gets a copy of the whole repository's tracked and unignored files, the
//! caller's uncommitted changes included. File names and diffs are bytes as git prints them, never decoded.
//!
//! The diff is taken relative to the workspace with plain `a/` and `b/` prefixes whatever the user's Git settings,
//! since cargo-mutants matches paths from the workspace: a diff relative to the repository root matches nothing and
//! reports success.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, FileTimes};
use std::io;
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::arguments::Revision;

/// A Git repository's working tree.
#[derive(Debug)]
pub(crate) struct Repository {
    root: PathBuf,
}

impl Repository {
    /// The repository whose working tree holds `workspace`, an absolute folder.
    ///
    /// # Errors
    /// [`RepositoryError::Git`] when git cannot run, and [`RepositoryError::NotInRepository`] when the folder is in no
    /// repository.
    pub(crate) fn holding(workspace: &Path) -> Result<Self, RepositoryError> {
        let top = git(workspace, ["rev-parse", "--show-toplevel"])?;
        if !top.status.success() {
            return Err(RepositoryError::NotInRepository {
                workspace: workspace.to_owned(),
                why: printed(&top.stderr),
            });
        }
        let root = OsString::from_vec(top.stdout.trim_ascii().to_vec());
        Ok(Self { root: root.into() })
    }

    /// `workspace`, an absolute folder inside the working tree, relative to its root.
    ///
    /// # Errors
    /// [`RepositoryError::Outside`] for a folder outside the working tree, as one reached through a link can be.
    pub(crate) fn relative(&self, workspace: &Path) -> Result<PathBuf, RepositoryError> {
        workspace
            .strip_prefix(&self.root)
            .map(Path::to_owned)
            .map_err(|_| RepositoryError::Outside {
                workspace: workspace.to_owned(),
                root: self.root.clone(),
            })
    }

    /// The working tree's tracked and untracked files git does not ignore, relative to its root, each that exists:
    /// a file deleted since the index was written, or a link to nothing, is left out.
    ///
    /// # Errors
    /// [`RepositoryError::Git`] when git cannot run, and [`RepositoryError::List`] when it cannot list the files.
    pub(crate) fn files(&self) -> Result<Vec<PathBuf>, RepositoryError> {
        let listed = git(
            &self.root,
            [
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?;
        if !listed.status.success() {
            return Err(RepositoryError::List {
                why: printed(&listed.stderr),
            });
        }
        Ok(listed
            .stdout
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(|name| PathBuf::from(OsStr::from_bytes(name)))
            .filter(|name| self.root.join(name).exists())
            .collect())
    }

    /// Copies the working tree's file `name` into `destination`, keeping its place, permissions and times; a link is
    /// copied as a link.
    ///
    /// # Errors
    /// [`CopyError`] naming the file that could not be copied.
    pub(crate) fn copy(&self, name: &Path, destination: &Path) -> Result<(), CopyError> {
        let from = self.root.join(name);
        let to = destination.join(name);
        copy_file(&from, &to).map_err(|source| CopyError { from, to, source })
    }
}

/// The diff of the Rust and everything else under `workspace` since `base`, relative to the workspace.
///
/// # Errors
/// [`RepositoryError::Git`] when git cannot run, and [`RepositoryError::Diff`] when it refuses the revision.
pub(crate) fn diff(workspace: &Path, base: &Revision) -> Result<Vec<u8>, RepositoryError> {
    let arguments: [&OsStr; 9] = [
        "diff".as_ref(),
        "--relative".as_ref(),
        "--no-color".as_ref(),
        "--no-ext-diff".as_ref(),
        "--src-prefix=a/".as_ref(),
        "--dst-prefix=b/".as_ref(),
        base.as_os_str(),
        "--".as_ref(),
        ".".as_ref(),
    ];
    let diff = git(workspace, arguments)?;
    if !diff.status.success() {
        return Err(RepositoryError::Diff {
            why: printed(&diff.stderr),
        });
    }
    Ok(diff.stdout)
}

/// The Rust files a diff changes, as its `+++ b/` lines name them.
pub(crate) fn changed_rust(diff: &[u8]) -> Vec<&OsStr> {
    diff.split(|byte| *byte == b'\n')
        .filter_map(|line| line.strip_prefix(b"+++ b/"))
        .filter(|name| name.ends_with(b".rs"))
        .map(OsStr::from_bytes)
        .collect()
}

/// Runs git in `folder` with its output captured.
fn git<I, S>(folder: &Path, arguments: I) -> Result<Output, RepositoryError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .args(arguments)
        .current_dir(folder)
        .output()
        .map_err(|source| RepositoryError::Git {
            folder: folder.to_owned(),
            source,
        })
}

/// What a failed git printed on standard error, without the surrounding whitespace.
fn printed(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr.trim_ascii()).into_owned()
}

/// Copies one file, or one link as a link, making its folder.
fn copy_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(folder) = to.parent() {
        fs::create_dir_all(folder)?;
    }
    let metadata = fs::symlink_metadata(from)?;
    if metadata.is_symlink() {
        return symlink(fs::read_link(from)?, to);
    }
    let mut source = File::open(from)?;
    let mut copied = File::create(to)?;
    io::copy(&mut source, &mut copied)?;
    copied.set_times(
        FileTimes::new()
            .set_accessed(metadata.accessed()?)
            .set_modified(metadata.modified()?),
    )?;
    copied.set_permissions(metadata.permissions())
}

/// Why the repository could not be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RepositoryError {
    #[error("could not run git in {}", .folder.display())]
    Git {
        folder: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not in a Git repository: {why}", .workspace.display())]
    NotInRepository { workspace: PathBuf, why: String },
    #[error("{} is not inside the repository at {}", .workspace.display(), .root.display())]
    Outside { workspace: PathBuf, root: PathBuf },
    #[error("git diff failed: {why}")]
    Diff { why: String },
    #[error("git ls-files failed: {why}")]
    List { why: String },
}

/// A file that could not be copied into a shard's copy of the repository.
#[derive(Debug, thiserror::Error)]
#[error("could not copy {} to {}", .from.display(), .to.display())]
pub(crate) struct CopyError {
    from: PathBuf,
    to: PathBuf,
    #[source]
    source: io::Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/crates/core/src/lib.rs b/crates/core/src/lib.rs
--- a/crates/core/src/lib.rs
+++ b/crates/core/src/lib.rs
@@ -1 +1 @@
-pub fn one() -> u8 { 1 }
+pub fn one() -> u8 { 2 }
diff --git a/crates/core/Cargo.toml b/crates/core/Cargo.toml
--- a/crates/core/Cargo.toml
+++ b/crates/core/Cargo.toml
@@ -1 +1 @@
-version = \"0.1.0\"
+version = \"0.2.0\"
diff --git a/crates/core/src/gone.rs b/crates/core/src/gone.rs
deleted file mode 100644
--- a/crates/core/src/gone.rs
+++ /dev/null
@@ -1 +0,0 @@
-pub fn gone() {}
";

    #[test]
    fn a_folder_in_no_repository_is_refused_with_gits_reason() {
        let folder = crate::scratch("outside");
        let refused = Repository::holding(&folder);
        let _ = fs::remove_dir_all(&folder);
        match refused {
            Err(RepositoryError::NotInRepository { workspace, why }) => {
                assert_eq!(workspace, folder);
                assert!(why.contains("not a git repository"), "{why}");
            }
            other => panic!("not refused as outside a repository: {other:?}"),
        }
    }

    #[test]
    fn a_changed_rust_file_is_named_from_the_workspace() {
        assert_eq!(
            changed_rust(DIFF.as_bytes()),
            [OsStr::new("crates/core/src/lib.rs")]
        );
    }

    #[test]
    fn a_diff_of_a_manifest_and_a_deleted_file_changes_no_rust() {
        let manifest_and_deleted = &DIFF[DIFF
            .find("diff --git a/crates/core/Cargo.toml")
            .expect("in the diff")..];
        assert_eq!(
            changed_rust(manifest_and_deleted.as_bytes()),
            Vec::<&OsStr>::new()
        );
    }
}
