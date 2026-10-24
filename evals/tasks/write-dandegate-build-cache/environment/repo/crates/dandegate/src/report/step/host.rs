//! The host programs a plan names, given to every command after its revision shell's own tools.
//!
//! At the start of a run, dandegate looks for each named program on its own `PATH` and links each one it finds into a
//! folder of the run's scratch that holds nothing else ([`link`]). A command then runs as `nix develop --unset PATH
//! <checkout> -c dandegate --host-programs <folder> -- <command>`. Inside the shell, that copy of dandegate appends the
//! folder to the `PATH` the shell set and replaces itself with the command ([`enter`]). A tool of the same name in the
//! revision's shell so comes first, and nothing else on the caller's `PATH` reaches the command. Since the copy
//! replaces itself, the command is still the process the supervisor started.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write as _};
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use camino::Utf8Path;

use crate::plan::HostProgram;

/// The argument that makes dandegate enter a command with the host programs, before the folder, `--` and the command.
pub(crate) const FLAG: &str = "--host-programs";

/// Link each of `programs` found on `path` into `folder`, a new folder, and give the programs not found. A relative or
/// empty folder of `path` is read from `current`, and every link names its program by an absolute path, which may hold
/// `:`.
///
/// # Errors
/// Why the folder cannot be made or given: it cannot be made, a link cannot be written, or its path holds `:`, which
/// `PATH` cannot hold.
pub(crate) fn link<'a>(
    programs: &'a [HostProgram],
    path: &OsStr,
    current: &Path,
    folder: &Utf8Path,
) -> Result<Vec<&'a HostProgram>, String> {
    if folder.as_str().contains(':') {
        return Err(format!(
            "its folder {folder} holds `:`, which PATH cannot hold"
        ));
    }
    let folders: Vec<PathBuf> = env::split_paths(path)
        .map(|entry| current.join(entry))
        .collect();
    let made = (|| -> io::Result<Vec<&'a HostProgram>> {
        fs::create_dir(folder)?;
        let mut missing = Vec::new();
        for program in programs {
            match super::find_in(&folders, program.as_str()) {
                Some(found) => {
                    symlink(found, folder.join(program.as_str()))?;
                }
                None => missing.push(program),
            }
        }
        Ok(missing)
    })();
    made.map_err(|error| format!("its folder {folder} cannot be made: {error}"))
}

/// Run `command` with `folder` after the folders of this process's `PATH`, in place of this process. It returns only
/// when the command cannot start, with the exit code a shell gives then.
pub(crate) fn enter(folder: &OsStr, command: &[OsString]) -> u8 {
    let mut errors = io::stderr();
    let Some((program, arguments)) = command.split_first() else {
        let _ = writeln!(errors, "dandegate: the host programs were given no command");
        return 127;
    };
    let path = match env::var_os("PATH").filter(|path| !path.is_empty()) {
        Some(mut path) => {
            path.push(":");
            path.push(folder);
            path
        }
        None => folder.to_owned(),
    };
    let error = Command::new(program)
        .args(arguments)
        .env("PATH", path)
        .exec();
    let _ = writeln!(errors, "dandegate: {}: {error}", program.to_string_lossy());
    if error.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use camino::Utf8PathBuf;

    use super::*;

    #[test]
    fn each_program_found_is_linked_by_an_absolute_path_and_each_one_not_found_is_given_back() {
        let temporary = Utf8PathBuf::try_from(env::temp_dir()).expect("a UTF-8 temporary folder");
        let root =
            crate::report::acquire(&temporary, "dandegate-host-").expect("a folder of its own");
        for (folder, name) in [
            ("first", "shared"),
            ("second", "shared"),
            ("second", "only"),
        ] {
            fs::create_dir_all(root.join(folder)).expect("a folder");
            let file = root.join(folder).join(name);
            fs::write(&file, "").expect("a file");
            fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).expect("executable");
        }
        let programs: Vec<HostProgram> = ["shared", "only", "absent"]
            .into_iter()
            .map(|name| HostProgram::new(name).expect("a name"))
            .collect();
        // The first folder is relative, read from the root as the current folder.
        let path = env::join_paths([Path::new("first"), root.join("second").as_std_path()])
            .expect("a PATH");
        let linked = root.join("linked");

        let missing = link(&programs, &path, root.as_std_path(), &linked).expect("linked");
        assert_eq!(missing, [&programs[2]]);
        assert_eq!(
            fs::read_link(linked.join("shared")).expect("a link"),
            root.join("first/shared").as_std_path()
        );
        assert_eq!(
            fs::read_link(linked.join("only")).expect("a link"),
            root.join("second/only").as_std_path()
        );
        let mut names: Vec<String> = fs::read_dir(&linked)
            .expect("the folder")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(names, ["only", "shared"]);
        assert!(link(&programs, &path, root.as_std_path(), &linked).is_err());
        assert!(link(&programs, &path, root.as_std_path(), &root.join("a:b")).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_folder_of_the_path_is_the_current_one_whose_own_path_may_hold_a_colon() {
        let temporary = Utf8PathBuf::try_from(env::temp_dir()).expect("a UTF-8 temporary folder");
        let root =
            crate::report::acquire(&temporary, "dandegate-host-").expect("a folder of its own");
        let current = root.join("current:folder");
        for name in ["here", "bin/there"] {
            let file = current.join(name);
            fs::create_dir_all(file.parent().expect("a folder")).expect("a folder");
            fs::write(&file, "").expect("a file");
            fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).expect("executable");
        }
        let programs: Vec<HostProgram> = ["here", "there"]
            .into_iter()
            .map(|name| HostProgram::new(name).expect("a name"))
            .collect();
        let linked = root.join("linked");

        let missing = link(
            &programs,
            OsStr::new(":bin"),
            current.as_std_path(),
            &linked,
        )
        .expect("linked");
        assert_eq!(missing, Vec::<&HostProgram>::new());
        assert_eq!(
            fs::read_link(linked.join("here")).expect("a link"),
            current.join("here").as_std_path()
        );
        assert_eq!(
            fs::read_link(linked.join("there")).expect("a link"),
            current.join("bin/there").as_std_path()
        );
        let _ = fs::remove_dir_all(&root);
    }
}
