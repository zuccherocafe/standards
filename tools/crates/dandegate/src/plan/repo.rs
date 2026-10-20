//! Paths inside the repository as the steps file writes them: a folder with a trailing `/`, a file without one, each
//! relative and plain. The after revision's tree, which the reader checks them against.

use std::collections::BTreeSet;
use std::fmt;
use std::ops::Bound;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};

/// Why a written path is not a repository path of its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PathProblem {
    #[error("is not a plain relative path inside the repository")]
    Outside,
    #[error("names a folder, so it ends in /")]
    FolderWithoutSlash,
    #[error("names a file, so it does not end in /")]
    FileWithSlash,
    #[error("names a folder in the after revision; write it with a trailing /")]
    TreeFolder,
    #[error("names a file in the after revision; write it without the trailing /")]
    TreeFile,
    #[error("names no folder in the after revision")]
    NoFolder,
}

/// `text`'s ordinary parts, when it has at least one and nothing else: no root, `.`, `..` or empty part.
fn plain(text: &str) -> Option<Utf8PathBuf> {
    let path = Utf8Path::new(text);
    let ordinary = path
        .components()
        .all(|part| matches!(part, Utf8Component::Normal(_)));
    // Components hide a doubled or leading `./` separator, so the rejoined parts must spell the text exactly.
    let rejoined: Utf8PathBuf = path.components().collect();
    (ordinary && !text.is_empty() && rejoined.as_str() == text).then_some(rejoined)
}

/// A folder of the repository, written with a trailing `/`; `./` is the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepoDir(Utf8PathBuf);

impl RepoDir {
    pub(super) fn parse(text: &str) -> Result<Self, PathProblem> {
        if text == "./" {
            return Ok(Self(Utf8PathBuf::from(".")));
        }
        if let Some(inside) = text.strip_suffix('/') {
            plain(inside).map(Self).ok_or(PathProblem::Outside)
        } else {
            plain(text).ok_or(PathProblem::Outside)?;
            Err(PathProblem::FolderWithoutSlash)
        }
    }

    /// The folder without its trailing `/`, `.` for the root, as commands and git take it.
    pub(crate) fn as_path(&self) -> &Utf8Path {
        &self.0
    }
}

impl fmt::Display for RepoDir {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/", self.0)
    }
}

/// A file of the repository, written without a trailing `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepoFile(Utf8PathBuf);

impl RepoFile {
    pub(super) fn parse(text: &str) -> Result<Self, PathProblem> {
        match text.strip_suffix('/') {
            Some(inside) => {
                plain(inside).ok_or(PathProblem::Outside)?;
                Err(PathProblem::FileWithSlash)
            }
            None => plain(text).map(Self).ok_or(PathProblem::Outside),
        }
    }

    pub(crate) fn as_path(&self) -> &Utf8Path {
        &self.0
    }
}

impl fmt::Display for RepoFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

/// A folder or a file of the repository, told apart by the trailing `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RepoPath {
    Dir(RepoDir),
    File(RepoFile),
}

impl RepoPath {
    pub(super) fn parse(text: &str) -> Result<Self, PathProblem> {
        if text.ends_with('/') {
            RepoDir::parse(text).map(Self::Dir)
        } else {
            RepoFile::parse(text).map(Self::File)
        }
    }

    pub(crate) fn as_path(&self) -> &Utf8Path {
        match self {
            Self::Dir(folder) => folder.as_path(),
            Self::File(file) => file.as_path(),
        }
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dir(folder) => folder.fmt(formatter),
            Self::File(file) => file.fmt(formatter),
        }
    }
}

/// The file that makes a folder a Godot project.
pub(crate) const GODOT_PROJECT: &str = "project.godot";

/// The after revision's files, and the folders among them holding a `Cargo.toml` with a `[workspace]` table.
#[derive(Debug, Default)]
pub(crate) struct Tree {
    files: BTreeSet<Utf8PathBuf>,
    workspaces: BTreeSet<Utf8PathBuf>,
}

impl Tree {
    /// `files` are paths relative to the repository; `workspaces` are folders, `.` for the root.
    pub(crate) fn new(
        files: impl IntoIterator<Item = Utf8PathBuf>,
        workspaces: impl IntoIterator<Item = Utf8PathBuf>,
    ) -> Self {
        Self {
            files: files.into_iter().collect(),
            workspaces: workspaces.into_iter().collect(),
        }
    }

    pub(super) fn holds_file(&self, path: &Utf8Path) -> bool {
        self.files.contains(path)
    }

    /// Whether any file lies below `folder`; the root holds every file.
    pub(super) fn holds_folder(&self, folder: &Utf8Path) -> bool {
        if folder == "." {
            return !self.files.is_empty();
        }
        self.files
            .range::<Utf8Path, _>((Bound::Included(folder), Bound::Unbounded))
            .take_while(|file| file.starts_with(folder))
            .any(|file| file != folder)
    }

    /// The folders holding a Cargo workspace, `.` for the root.
    pub(super) fn workspaces(&self) -> impl Iterator<Item = &Utf8Path> {
        self.workspaces.iter().map(Utf8PathBuf::as_path)
    }

    pub(super) fn holds_workspace(&self, folder: &RepoDir) -> bool {
        self.workspaces.contains(folder.as_path())
    }

    /// The folders holding a Godot project's `project.godot`, `.` for the root.
    pub(super) fn godot_projects(&self) -> impl Iterator<Item = &Utf8Path> {
        self.files
            .iter()
            .filter(|file| file.file_name() == Some(GODOT_PROJECT))
            .map(|file| match file.parent() {
                Some(folder) if !folder.as_str().is_empty() => folder,
                _ => Utf8Path::new("."),
            })
    }

    /// `written` checked against the tree: a folder written as a file, or a file as a folder, is refused. An absent
    /// path passes, since the field may name what the change removes.
    pub(super) fn check(&self, written: &RepoPath) -> Result<(), PathProblem> {
        match written {
            RepoPath::File(file) if self.holds_folder(file.as_path()) => {
                Err(PathProblem::TreeFolder)
            }
            RepoPath::Dir(folder) if self.holds_file(folder.as_path()) => {
                Err(PathProblem::TreeFile)
            }
            RepoPath::Dir(_) | RepoPath::File(_) => Ok(()),
        }
    }
}
