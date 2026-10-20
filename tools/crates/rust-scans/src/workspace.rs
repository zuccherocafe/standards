//! The tool's only process and file boundary: the workspace's members as `cargo metadata` lists them, and their source
//! files read and parsed.
//!
//! Members are what Cargo calls the workspace's members, so a root package, `exclude` and path members follow Cargo's
//! own rules rather than a folder layout. Every `.rs` file below a member's `src/` is read, whether or not a target
//! reaches it, since an unreached file is still code someone reviews.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;
use std::{fs, io};

use camino::{Utf8Path, Utf8PathBuf};

use crate::options::{BadName, CrateName};

/// A Cargo workspace's member packages, in folder order.
#[derive(Debug)]
pub(crate) struct Workspace {
    members: Vec<Member>,
}

/// One member package: its name, its folder and its targets' root files.
#[derive(Debug)]
pub(crate) struct Member {
    name: CrateName,
    folder: Utf8PathBuf,
    roots: Vec<Utf8PathBuf>,
}

impl Workspace {
    /// The members of the workspace whose `Cargo.toml` is in `folder`, from `cargo metadata --no-deps`.
    ///
    /// # Errors
    /// [`MetadataError`] when Cargo cannot be started, fails, or prints something other than its metadata.
    pub(crate) fn discover(folder: &Utf8Path) -> Result<Self, MetadataError> {
        let output = Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(folder)
            .output()
            .map_err(|source| MetadataError::Launch {
                workspace: folder.to_owned(),
                source,
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(MetadataError::Failed {
                workspace: folder.to_owned(),
                stderr,
            });
        }
        let metadata: cargo_metadata::Metadata =
            serde_json::from_slice(&output.stdout).map_err(MetadataError::Malformed)?;
        let mut members = metadata
            .workspace_packages()
            .into_iter()
            .map(|package| {
                let name = CrateName::new(&package.name)?;
                let folder = package
                    .manifest_path
                    .parent()
                    .unwrap_or(&package.manifest_path)
                    .to_owned();
                let roots = package
                    .targets
                    .iter()
                    .map(|target| target.src_path.clone())
                    .collect();
                Ok(Member {
                    name,
                    folder,
                    roots,
                })
            })
            .collect::<Result<Vec<_>, BadName>>()?;
        members.sort_by(|left, right| left.folder.cmp(&right.folder));
        Ok(Self { members })
    }

    pub(crate) fn members(&self) -> &[Member] {
        &self.members
    }
}

impl Member {
    pub(crate) fn name(&self) -> &CrateName {
        &self.name
    }

    /// The member's `src/` folder.
    pub(crate) fn src(&self) -> Utf8PathBuf {
        self.folder.join("src")
    }

    /// The root file of each of the member's targets, such as `src/lib.rs`.
    pub(crate) fn roots(&self) -> &[Utf8PathBuf] {
        &self.roots
    }
}

/// Every `.rs` file below `src`, in path order; none when the member has no `src/`.
///
/// # Errors
/// [`ReadError::Io`] for a folder that exists but cannot be listed, and [`ReadError::NotUtf8`] for a folder or `.rs`
/// file whose path is not UTF-8.
pub(crate) fn sources(src: &Utf8Path) -> Result<Vec<Utf8PathBuf>, ReadError> {
    let mut found = Vec::new();
    if src.is_dir() {
        walk(src, &mut found)?;
    }
    found.sort();
    Ok(found)
}

fn walk(folder: &Utf8Path, found: &mut Vec<Utf8PathBuf>) -> Result<(), ReadError> {
    let failed = |source| ReadError::Io {
        file: folder.to_owned(),
        source,
    };
    for entry in fs::read_dir(folder).map_err(failed)? {
        let entry = entry.map_err(failed)?;
        let path = entry.path();
        let utf8 = |path: PathBuf| {
            Utf8PathBuf::try_from(path).map_err(|error| ReadError::NotUtf8(error.into_path_buf()))
        };
        // A source that cannot be read, such as a broken link, is listed here and refused where it is read.
        if entry.file_type().map_err(failed)?.is_dir() {
            walk(&utf8(path)?, found)?;
        } else if path.extension() == Some(OsStr::new("rs")) {
            found.push(utf8(path)?);
        }
    }
    Ok(())
}

/// The Rust syntax of the file at `path`.
///
/// # Errors
/// [`ReadError`] when the file cannot be read as UTF-8 text or does not parse as Rust.
pub(crate) fn parse(path: &Utf8Path) -> Result<syn::File, ReadError> {
    let text = fs::read_to_string(path).map_err(|source| ReadError::Io {
        file: path.to_owned(),
        source,
    })?;
    syn::parse_file(&text).map_err(|source| {
        let at = source.span().start();
        ReadError::Parse {
            file: path.to_owned(),
            line: at.line,
            column: at.column + 1,
            source,
        }
    })
}

/// Why the workspace's members could not be listed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MetadataError {
    #[error("could not run cargo metadata in {workspace}")]
    Launch {
        workspace: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cargo metadata failed in {workspace}: {stderr}")]
    Failed {
        workspace: Utf8PathBuf,
        stderr: String,
    },
    #[error("cargo metadata printed something other than its metadata")]
    Malformed(#[source] serde_json::Error),
    #[error("cargo metadata listed a member whose name is not a crate name")]
    Name(#[from] BadName),
}

/// Why a source file could not be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    #[error("could not read {file}")]
    Io {
        file: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not read {}, whose path is not UTF-8", .0.display())]
    NotUtf8(PathBuf),
    #[error("could not parse {file}:{line}:{column} as Rust")]
    Parse {
        file: Utf8PathBuf,
        line: usize,
        column: usize,
        #[source]
        source: syn::Error,
    },
}
