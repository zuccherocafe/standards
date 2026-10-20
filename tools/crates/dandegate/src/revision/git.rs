//! Reads and changes the repository only through the `git` command. It resolves revisions, reads a file at a commit,
//! lists the files that differ between two commits, and adds and removes the detached worktrees that steps run in.

use std::collections::BTreeMap;
use std::process::{Command, Stdio};

use camino::{Utf8Path, Utf8PathBuf};

use crate::revision::{CommitId, Revision};

/// The repository's top folder, where `git` runs.
#[derive(Clone, Debug)]
pub(crate) struct Repository {
    root: Utf8PathBuf,
}

/// A `git` command that could not run or failed.
#[derive(Debug, thiserror::Error)]
#[error("`git {command}` failed: {why}")]
pub(crate) struct GitError {
    command: String,
    why: String,
}

/// Why a revision, or the repository itself, could not be resolved.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ResolveError {
    #[error("dandegate runs inside a git repository")]
    NotRepository(#[source] GitError),
    #[error("the repository's folder is not UTF-8: {0}")]
    NotUtf8(String),
    #[error("{requested:?} names no commit")]
    Unresolved {
        requested: String,
        #[source]
        cause: GitError,
    },
}

impl Repository {
    /// The repository holding the folder `folder`.
    ///
    /// # Errors
    /// [`ResolveError::NotRepository`] outside a git repository.
    pub(crate) fn discover(folder: &Utf8Path) -> Result<Self, ResolveError> {
        let printed = git_in(folder, &["rev-parse", "--show-toplevel"])
            .map_err(ResolveError::NotRepository)?;
        let root =
            String::from_utf8(printed).map_err(|error| ResolveError::NotUtf8(error.to_string()))?;
        Ok(Self {
            root: Utf8PathBuf::from(root.trim_end_matches('\n')),
        })
    }

    pub(crate) fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// The commit `requested` names now.
    ///
    /// # Errors
    /// [`ResolveError::Unresolved`] when it names no commit.
    pub(crate) fn resolve(&self, requested: &str) -> Result<Revision, ResolveError> {
        let unresolved = |cause| ResolveError::Unresolved {
            requested: requested.to_owned(),
            cause,
        };
        let name = format!("{requested}^{{commit}}");
        let printed = self
            .git(&[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &name,
            ])
            .map_err(unresolved)?;
        let text = String::from_utf8_lossy(&printed);
        let commit = CommitId::new(text.trim_end()).map_err(|error| {
            unresolved(GitError {
                command: "rev-parse".to_owned(),
                why: error.to_string(),
            })
        })?;
        Ok(Revision::new(requested.to_owned(), commit))
    }

    /// The file at `path` in `commit`'s tree, or none where the tree has no such file.
    ///
    /// # Errors
    /// [`GitError`] when the tree cannot be read.
    pub(crate) fn file_at(
        &self,
        commit: &CommitId,
        path: &str,
    ) -> Result<Option<Vec<u8>>, GitError> {
        let listed = self.git(&["ls-tree", "-z", commit.as_str(), "--", path])?;
        if listed.is_empty() {
            return Ok(None);
        }
        self.git(&["cat-file", "blob", &format!("{commit}:{path}")])
            .map(Some)
    }

    /// Every file in `commit`'s tree under `folder`, by its path there, with its blob's object name, which changes
    /// exactly when its content does.
    ///
    /// # Errors
    /// [`GitError`] when the tree cannot be read.
    pub(crate) fn blobs(
        &self,
        commit: &CommitId,
        folder: &str,
    ) -> Result<BTreeMap<String, String>, GitError> {
        let listed = self.git(&["ls-tree", "-r", "-z", commit.as_str(), "--", folder])?;
        let mut found = BTreeMap::new();
        for entry in String::from_utf8_lossy(&listed)
            .split('\0')
            .filter(|entry| !entry.is_empty())
        {
            if let Some((meta, path)) = entry.split_once('\t')
                && let [_, "blob", object] = meta.split(' ').collect::<Vec<_>>()[..]
            {
                found.insert(path.to_owned(), object.to_owned());
            }
        }
        Ok(found)
    }

    /// The paths of every file `commit`'s tree holds.
    ///
    /// # Errors
    /// [`GitError`] when the tree cannot be read.
    pub(crate) fn files(&self, commit: &CommitId) -> Result<Vec<String>, GitError> {
        let listed = self.git(&["ls-tree", "-r", "-z", "--name-only", commit.as_str()])?;
        Ok(String::from_utf8_lossy(&listed)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// The files that differ between two commits under any of `paths`, every path when none is given. Each path is
    /// literal, a file or a folder with everything below it, never a pattern: git's wildcards, brackets and pathspec
    /// magic match only themselves.
    ///
    /// # Errors
    /// [`GitError`] when either commit cannot be read.
    pub(crate) fn changed(
        &self,
        before: &CommitId,
        after: &CommitId,
        paths: &[&str],
    ) -> Result<Vec<String>, GitError> {
        let mut arguments = vec![
            "--literal-pathspecs",
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            before.as_str(),
            after.as_str(),
            "--",
        ];
        arguments.extend_from_slice(paths);
        let listed = self.git(&arguments)?;
        Ok(String::from_utf8_lossy(&listed)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// Check `commit` out, detached, at `folder`, which must not exist.
    ///
    /// # Errors
    /// [`GitError`] when the worktree cannot be added.
    pub(crate) fn add_worktree(
        &self,
        folder: &Utf8Path,
        commit: &CommitId,
    ) -> Result<(), GitError> {
        self.git(&[
            "worktree",
            "add",
            "--detach",
            "--quiet",
            folder.as_str(),
            commit.as_str(),
        ])
        .map(drop)
    }

    /// Remove the worktree at `folder`, with whatever its steps wrote.
    ///
    /// # Errors
    /// [`GitError`] when git refuses.
    pub(crate) fn remove_worktree(&self, folder: &Utf8Path) -> Result<(), GitError> {
        self.git(&["worktree", "remove", "--force", "--force", folder.as_str()])
            .map(drop)
    }

    /// Forget worktrees whose folders are gone.
    ///
    /// # Errors
    /// [`GitError`] when git refuses.
    pub(crate) fn prune_worktrees(&self) -> Result<(), GitError> {
        self.git(&["worktree", "prune"]).map(drop)
    }

    fn git(&self, arguments: &[&str]) -> Result<Vec<u8>, GitError> {
        git_in(&self.root, arguments)
    }
}

/// What `git` prints run in `folder`.
fn git_in(folder: &Utf8Path, arguments: &[&str]) -> Result<Vec<u8>, GitError> {
    let failed = |why: String| GitError {
        command: arguments.join(" "),
        why,
    };
    let done = Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| failed(error.to_string()))?;
    if done.status.success() {
        Ok(done.stdout)
    } else {
        let why = String::from_utf8_lossy(&done.stderr).trim().to_owned();
        Err(failed(if why.is_empty() {
            done.status.to_string()
        } else {
            why
        }))
    }
}
