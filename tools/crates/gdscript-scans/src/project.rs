//! The Godot project's files on disk: every script, scene, resource and `project.godot` below the project folder,
//! each marked test or not, and the autoloads `project.godot` names. This is the crate's one reader of the file system.

use std::collections::BTreeMap;
use std::fs;
use std::io;

use camino::{Utf8Path, Utf8PathBuf};

use crate::lexer::Dialect;

/// One file the scans read.
#[derive(Debug)]
pub(crate) struct ProjectFile {
    /// Relative to the project folder.
    pub(crate) path: Utf8PathBuf,
    /// The text, or none when it is not UTF-8.
    pub(crate) text: Option<String>,
    /// Whether it lies in one of the test folders.
    pub(crate) test: bool,
}

impl ProjectFile {
    pub(crate) fn dialect(&self) -> Dialect {
        if self.is_script() {
            Dialect::Script
        } else {
            Dialect::Resource
        }
    }

    pub(crate) fn is_script(&self) -> bool {
        self.path.extension() == Some("gd")
    }
}

/// Why the project cannot be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ProjectError {
    #[error("{0} is not a folder")]
    NotFolder(Utf8PathBuf),
    #[error("the test folder {tests} is not inside the project {project}")]
    TestsOutside {
        tests: Utf8PathBuf,
        project: Utf8PathBuf,
    },
    #[error("could not read {path}")]
    Read {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
}

/// The file names the scans read: scripts, and the scenes, resources and project file that name their members.
fn wanted(path: &Utf8Path) -> bool {
    matches!(path.extension(), Some("gd" | "tscn" | "tres" | "godot"))
}

/// Every file the scans read below `project`, in path order, with each one under a folder of `tests` marked.
/// Folders whose names start with `.`, folders holding a `.gdignore`, which Godot does not import, and symbolic
/// links are skipped.
///
/// # Errors
/// [`ProjectError`] when a folder is not one, a test folder lies outside the project, or a file cannot be read.
pub(crate) fn files(
    project: &Utf8Path,
    tests: &[Utf8PathBuf],
) -> Result<Vec<ProjectFile>, ProjectError> {
    let root = canonical(project)?;
    let mut test_folders = Vec::new();
    for folder in tests {
        let canonical = canonical(folder)?;
        let inside = canonical
            .strip_prefix(&root)
            .map_err(|_| ProjectError::TestsOutside {
                tests: folder.clone(),
                project: project.to_owned(),
            })?;
        test_folders.push(inside.to_owned());
    }
    let mut found = Vec::new();
    walk(&root, Utf8Path::new(""), &test_folders, &mut found)?;
    found.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(found)
}

fn canonical(folder: &Utf8Path) -> Result<Utf8PathBuf, ProjectError> {
    let not_folder = || ProjectError::NotFolder(folder.to_owned());
    let resolved = fs::canonicalize(folder).map_err(|_| not_folder())?;
    let resolved = Utf8PathBuf::from_path_buf(resolved).map_err(|_| not_folder())?;
    if resolved.is_dir() {
        Ok(resolved)
    } else {
        Err(not_folder())
    }
}

fn walk(
    root: &Utf8Path,
    inside: &Utf8Path,
    tests: &[Utf8PathBuf],
    found: &mut Vec<ProjectFile>,
) -> Result<(), ProjectError> {
    let folder = root.join(inside);
    let read = |source| ProjectError::Read {
        path: folder.clone(),
        source,
    };
    if folder.join(".gdignore").exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&folder).map_err(read)? {
        let entry = entry.map_err(read)?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let kind = entry.file_type().map_err(read)?;
        let path = inside.join(&name);
        if kind.is_dir() {
            walk(root, &path, tests, found)?;
        } else if kind.is_file() && wanted(&path) {
            let bytes = fs::read(root.join(&path)).map_err(|source| ProjectError::Read {
                path: root.join(&path),
                source,
            })?;
            found.push(ProjectFile {
                test: tests.iter().any(|folder| path.starts_with(folder)),
                text: String::from_utf8(bytes).ok(),
                path,
            });
        }
    }
    Ok(())
}

/// The autoloads the `[autoload]` section of `project.godot`'s text names: each script's path relative to the project,
/// with its name. A scene autoload names no script. A `;` outside quotes starts a comment.
pub(crate) fn autoloads(project_godot: &str) -> BTreeMap<Utf8PathBuf, String> {
    let mut found = BTreeMap::new();
    let mut in_section = false;
    for line in project_godot.lines().map(|line| uncommented(line).trim()) {
        if line.starts_with('[') {
            in_section = line == "[autoload]";
            continue;
        }
        let Some((name, value)) = line.split_once('=').filter(|_| in_section) else {
            continue;
        };
        let Some(quoted) = value
            .trim()
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        else {
            continue;
        };
        let path = quoted.trim_start_matches('*');
        if let Some(inside) = path
            .strip_prefix("res://")
            .filter(|inside| Utf8Path::new(inside).extension() == Some("gd"))
        {
            found.insert(Utf8PathBuf::from(inside), name.trim().to_owned());
        }
    }
    found
}

/// A line of `project.godot` up to its comment: a `;` outside a quoted value.
fn uncommented(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        match character {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ';' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_after_a_section_or_a_value_is_skipped_and_a_quoted_semicolon_is_kept() {
        let text = "[autoload] ; services\nRegistry=\"*res://registry.gd\" ; global service\n\
                    Odd=\"*res://odd;name.gd\"\nEscaped=\"*res://say \\\"hi;there\\\".gd\" ; comment\n";
        let expected: BTreeMap<Utf8PathBuf, String> = [
            ("registry.gd", "Registry"),
            ("odd;name.gd", "Odd"),
            ("say \\\"hi;there\\\".gd", "Escaped"),
        ]
        .into_iter()
        .map(|(path, name)| (Utf8PathBuf::from(path), name.to_owned()))
        .collect();
        assert_eq!(autoloads(text), expected);
    }

    #[test]
    fn an_autoload_is_named_by_its_key_and_only_scripts_count() {
        let text = "[application]\nname=\"x\"\n[autoload]\nDollRegistry=\"*res://addons/dolls/registry.gd\"\n\
                    Scene=\"*res://scene.tscn\"\nPlain=\"res://plain.gd\"\n[display]\nOther=\"*res://other.gd\"\n";
        let found = autoloads(text);
        let expected: BTreeMap<Utf8PathBuf, String> = [
            ("addons/dolls/registry.gd", "DollRegistry"),
            ("plain.gd", "Plain"),
        ]
        .into_iter()
        .map(|(path, name)| (Utf8PathBuf::from(path), name.to_owned()))
        .collect();
        assert_eq!(found, expected);
    }
}
