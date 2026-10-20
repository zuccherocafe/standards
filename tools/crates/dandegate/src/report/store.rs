//! Keeps each complete, successful step's folder in the repository's ignored `.cache/dandegate/results/`, under the
//! identity of everything the result depends on. A later report of the same revisions reuses the kept result instead of
//! running the step again. A step kept by content is found by what it reads instead ([`Identity`]).
//!
//! An entry is a folder named by its identity's digest. It holds `receipt.json`, with a format number, the identity and
//! every file's SHA-256, and `files/`, a copy of the step's folder. dandegate writes an entry under a staging name that
//! starts with a dot, then renames it into place whole. An entry in place is therefore never partial, and dandegate
//! never reads a staging folder. dandegate uses an entry only when its receipt has this format, names exactly the
//! identity asked for, and lists exactly the files present, with their digests. A missing, changed or extra file, a
//! malformed receipt, or another format is a miss, and the step runs again. dandegate never keeps a build tree or a
//! Cargo target.
//!
//! When dandegate cannot read or write the store, it loses only the reuse. The page says so, and dandegate writes the
//! page all the same.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read as _};

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::revision::CommitId;

/// The receipts' format. A receipt in any other format is a miss.
const FORMAT: u32 = 1;

/// A SHA-256 as lowercase hexadecimal.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// A file's SHA-256 as lowercase hexadecimal.
///
/// # Errors
/// The file's read error.
pub(crate) fn file_sha256(path: &Utf8Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut file = fs::File::open(path)?;
    let mut block = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut block)?;
        if read == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&block[..read]);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            // Writing to a String cannot fail.
            let _ = std::fmt::Write::write_fmt(&mut text, format_args!("{byte:02x}"));
            text
        })
}

/// What a result follows from of the two revisions: its revision's commit, both of a once step's, or, for a step kept
/// by content, the digests of the content it reads, so a new commit of the same content still finds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "runs", rename_all = "kebab-case")]
pub(crate) enum Revisions {
    Revision {
        commit: String,
    },
    Pair {
        before: String,
        after: String,
    },
    /// The SHA-256 of the diff the step reads, and of the after revision's tree listing outside the adapters folder.
    Content {
        diff: String,
        tree: String,
    },
}

impl Revisions {
    pub(crate) fn revision(commit: &CommitId) -> Self {
        Self::Revision {
            commit: commit.to_string(),
        }
    }

    pub(crate) fn pair(before: &CommitId, after: &CommitId) -> Self {
        Self::Pair {
            before: before.to_string(),
            after: after.to_string(),
        }
    }
}

/// An input's value as an identity holds it, with the content of the file it names, if it names one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct InputIdentity {
    pub(crate) value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file: Option<String>,
}

/// What one instance adds to the identity of each result that follows from it: the environment variables its step's
/// patterns match, and its collector's lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Contribution {
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) collected: BTreeMap<String, String>,
}

/// A step that a result needs, as the step runs at one place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct NeedIdentity {
    pub(crate) step: serde_json::Value,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) collected: BTreeMap<String, String>,
    /// The CPUs it runs on, left out for a step whose result does not follow them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cpus: Option<String>,
}

/// Everything a kept result depends on:
///
/// - The commits, which also fix each revision's development shell through its `flake.lock`. A step kept by content,
///   the mutation report, holds the digests of the diff it reads and of the after revision's tree instead: every file
///   but the adapters', the `flake.lock` included. A report of other commits with the same content reuses it.
/// - The step as written, with placeholders by name.
/// - Each instance it needs, directly or through others, which a reused result skips: by place and name, its step as
///   written, the environment variables its patterns match and its collector's lines.
/// - The input values those steps use, with the content of any file a value names.
/// - Every file of the after revision's adapters folder, except for a step kept by content, which runs none of them.
/// - The environment variables the step's patterns match, and its collector's lines.
/// - dandegate's own executable.
/// - The host programs the plan names, by name: the plan comes from the after revision, so the before commit does not
///   fix them.
/// - For a built-in report, the settings of the steps file it reads: for a Rust report, the workspace and each report's
///   arguments.
/// - For the mutation report, whose job budget follows them, and for a step that names `{cpus}`, the CPUs it runs on;
///   the same for each instance it needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Identity {
    pub(crate) dandegate: String,
    pub(crate) revisions: Revisions,
    pub(crate) step: serde_json::Value,
    pub(crate) needs: BTreeMap<String, NeedIdentity>,
    pub(crate) inputs: BTreeMap<String, InputIdentity>,
    pub(crate) adapters: BTreeMap<String, String>,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) collected: BTreeMap<String, String>,
    /// The host programs the plan names, as written, left out when it names none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) host: Vec<String>,
    /// The settings a built-in report reads, left out for a step that is not one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) settings: Option<serde_json::Value>,
    /// The CPUs the step runs on, left out for a step whose result does not follow them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cpus: Option<String>,
}

impl Identity {
    /// The identity as its receipt holds it.
    fn written(&self) -> serde_json::Value {
        // A record of strings and maps always serialises.
        serde_json::to_value(self).unwrap_or_default()
    }

    /// The name of the entry kept under this identity: its canonical text's digest.
    fn digest(&self) -> String {
        sha256(self.written().to_string().as_bytes())
    }
}

/// A kept entry's receipt.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    format: u32,
    identity: serde_json::Value,
    files: BTreeMap<String, String>,
}

/// Why a result could not be kept.
#[derive(Debug, thiserror::Error)]
#[error("could not keep a result in {folder}")]
pub(crate) struct StoreError {
    folder: Utf8PathBuf,
    #[source]
    source: io::Error,
}

/// What a look-up found.
#[derive(Debug)]
pub(crate) enum Lookup {
    /// A whole entry's folder.
    Hit(Utf8PathBuf),
    /// Nothing usable, and why.
    Miss(String),
}

/// The kept results under one folder.
#[derive(Debug)]
pub(crate) struct Store {
    folder: Utf8PathBuf,
}

impl Store {
    pub(crate) fn new(folder: Utf8PathBuf) -> Self {
        Self { folder }
    }

    /// The entry kept under `identity`, when it is whole.
    pub(crate) fn find(&self, identity: &Identity) -> Lookup {
        let entry = self.folder.join(identity.digest());
        if !entry.is_dir() {
            return Lookup::Miss("nothing is kept for it".to_owned());
        }
        let receipt = match fs::read_to_string(entry.join("receipt.json")) {
            Err(error) => return Lookup::Miss(format!("its receipt cannot be read: {error}")),
            Ok(text) => match serde_json::from_str::<Receipt>(&text) {
                Err(error) => return Lookup::Miss(format!("its receipt is not one: {error}")),
                Ok(receipt) => receipt,
            },
        };
        if receipt.format != FORMAT {
            return Lookup::Miss(format!(
                "its receipt is in format {}, not {FORMAT}",
                receipt.format
            ));
        }
        if receipt.identity != identity.written() {
            return Lookup::Miss("it was kept under another identity".to_owned());
        }
        if listed(&entry.join("files")).ok().as_ref() != Some(&receipt.files) {
            return Lookup::Miss("its files are not the ones its receipt lists".to_owned());
        }
        Lookup::Hit(entry)
    }

    /// Copy a whole entry's files into `into`, a new folder. If the copy fails, it removes whatever it copied.
    ///
    /// # Errors
    /// The copy's error.
    pub(crate) fn restore(entry: &Utf8Path, into: &Utf8Path) -> io::Result<()> {
        let copied = copy_folder(&entry.join("files"), into);
        if copied.is_err() {
            // A partial copy would read as the step's own output.
            let _ = fs::remove_dir_all(into);
        }
        copied
    }

    /// Keep a copy of the folder `files` under `identity`, replacing what was kept there.
    ///
    /// # Errors
    /// [`StoreError`] when the store cannot be written. It never leaves a partial entry in place.
    pub(crate) fn keep(&self, identity: &Identity, files: &Utf8Path) -> Result<(), StoreError> {
        let failed = |source| StoreError {
            folder: self.folder.clone(),
            source,
        };
        let digest = identity.digest();
        let entry = self.folder.join(&digest);
        // A staging folder is made by this call, so a run in another PID namespace keeping the same identity has
        // its own.
        let staging = super::acquire(&self.folder, &format!(".{digest}-")).map_err(failed)?;
        let written = (|| -> io::Result<()> {
            copy_folder(files, &staging.join("files"))?;
            let receipt = Receipt {
                format: FORMAT,
                identity: identity.written(),
                files: listed(&staging.join("files"))?,
            };
            let text = serde_json::to_string_pretty(&receipt).map_err(io::Error::other)?;
            fs::write(staging.join("receipt.json"), text + "\n")?;
            if entry.exists() {
                fs::remove_dir_all(&entry)?;
            }
            fs::rename(&staging, &entry)
        })();
        if written.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        written.map_err(failed)
    }
}

/// Every file under `folder`, by its path there, with its digest. An error for anything that is neither a plain file
/// nor a folder.
fn listed(folder: &Utf8Path) -> io::Result<BTreeMap<String, String>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![folder.to_owned()];
    while let Some(current) = pending.pop() {
        for entry in current.read_dir_utf8()? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(entry.path().to_owned());
            } else if kind.is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(folder)
                    .map_err(io::Error::other)?;
                found.insert(relative.to_string(), file_sha256(entry.path())?);
            } else {
                return Err(io::Error::other(format!(
                    "{} is neither a file nor a folder",
                    entry.path()
                )));
            }
        }
    }
    Ok(found)
}

/// Copy the folder `from` to the new folder `to`, plain files and folders only.
///
/// # Errors
/// Any read or write error, or an entry that is neither a file nor a folder.
pub(crate) fn copy_folder(from: &Utf8Path, to: &Utf8Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in from.read_dir_utf8()? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_folder(entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        } else {
            return Err(io::Error::other(format!(
                "{} is neither a file nor a folder",
                entry.path()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_is_sha256_in_lowercase_hexadecimal() {
        // The SHA-256 of "abc".
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
