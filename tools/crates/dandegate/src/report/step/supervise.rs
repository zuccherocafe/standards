//! The process that owns one command's whole process tree.
//!
//! dandegate starts every command through a copy of itself, `dandegate --supervise [--cpus <list>] -- <command>`, which
//! becomes the command's child subreaper: every process the command starts stays below it, whether it starts a new
//! session or is left behind when its parent exits. The supervisor reads its standard input, a pipe from dandegate,
//! until it ends. dandegate closes the pipe when the command's time runs out, and the kernel closes it when dandegate
//! ends for any reason, a signal included.
//!
//! Given `--cpus`, the supervisor runs on those CPUs before it starts the command, so the command and everything it
//! starts run on them too. When it cannot, it starts nothing and fails as a command that cannot start does.
//!
//! The supervisor stops its tree when the command exits or the pipe ends, whichever comes first. It kills every process
//! below it and reaps each one before it exits, so nothing the command started outlives it. It then exits as the
//! command did, with its exit code or by the signal that ended it.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read as _, Write as _};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use rustix::process::{
    Pid, PidfdFlags, Signal, WaitOptions, getpid, kill_process, pidfd_open, pidfd_send_signal,
    set_child_subreaper, wait,
};

use crate::report::cpus::Cpus;

/// The argument that makes dandegate a supervisor, before `--` and the command.
pub(crate) const FLAG: &str = "--supervise";

/// The supervisor's option that names the CPUs the command runs on, as `taskset -c` writes them.
pub(crate) const CPUS: &str = "--cpus";

/// How often the supervisor looks for the command's end and the pipe's: the same as dandegate's own check.
const POLL: Duration = super::POLL;

/// The exit code when the command cannot be started, as a shell gives for a command it cannot find.
const NOT_STARTED: u8 = 127;

/// Run `command`, on `cpus` when given, to its end or the pipe's, stop everything it started, and give the command's
/// exit code. When a signal ended the command, the supervisor ends by the same signal first.
pub(crate) fn supervise(cpus: Option<&OsStr>, command: &[OsString]) -> u8 {
    let mut errors = io::stderr();
    if let Some(written) = cpus {
        let entered = written
            .to_str()
            .and_then(Cpus::parse)
            .ok_or_else(|| format!("{} is not a list of CPUs", written.to_string_lossy()))
            .and_then(|cpus| cpus.enter().map_err(|error| error.to_string()));
        if let Err(why) = entered {
            let _ = writeln!(errors, "dandegate: the step's CPUs cannot be set: {why}");
            return NOT_STARTED;
        }
    }
    // Without the subreaper attribute an orphan escapes to init. The command still runs and is still stopped.
    if let Err(error) = set_child_subreaper(Some(getpid())) {
        let _ = writeln!(
            errors,
            "dandegate: the step's processes cannot all be owned: {error}"
        );
    }
    let Some((program, arguments)) = command.split_first() else {
        let _ = writeln!(errors, "dandegate: the supervisor was given no command");
        return NOT_STARTED;
    };
    let leader = match Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(child) => Pid::from_child(&child),
        Err(error) => {
            let _ = writeln!(
                errors,
                "dandegate: {} could not be started: {error}",
                program.to_string_lossy()
            );
            return NOT_STARTED;
        }
    };
    let closed = Arc::new(AtomicBool::new(false));
    let watcher = Arc::clone(&closed);
    thread::spawn(move || {
        let mut sink = [0_u8; 64];
        let mut input = io::stdin();
        // Whatever ends the read, an end of input or an error, the pipe no longer says to go on.
        while matches!(input.read(&mut sink), Ok(read) if read > 0) {}
        watcher.store(true, Ordering::SeqCst);
    });
    let mut ended = None;
    while ended.is_none() && !closed.load(Ordering::SeqCst) {
        ended = reap(Some(leader));
        if ended.is_none() {
            thread::sleep(POLL);
        }
    }
    stop_descendants();
    match ended {
        Some(Ended::Exited(code)) => code,
        Some(Ended::Signalled(signal)) => {
            if let Some(signal) = Signal::from_named_raw(signal) {
                let _ = kill_process(getpid(), signal);
            }
            // A signal whose default is not to end a process leaves the supervisor running to here.
            u8::try_from(128 + signal).unwrap_or(u8::MAX)
        }
        // The pipe ended first: dandegate has stopped the step or has ended itself.
        None => 1,
    }
}

/// How the command ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ended {
    Exited(u8),
    Signalled(i32),
}

/// Reap every child that has ended, and give how `leader` ended if it was one of them.
fn reap(leader: Option<Pid>) -> Option<Ended> {
    let mut ended = None;
    while let Ok(Some((pid, status))) = wait(WaitOptions::NOHANG) {
        if Some(pid) == leader {
            ended = match (status.exit_status(), status.terminating_signal()) {
                (Some(code), _) => Some(Ended::Exited(u8::try_from(code).unwrap_or(u8::MAX))),
                (None, Some(signal)) => Some(Ended::Signalled(signal)),
                (None, None) => None,
            };
        }
    }
    ended
}

/// Kill every process below this one and reap each, until none is left. A process killed here starts no other, and
/// the children of a process killed here become this one's, so the loop ends.
fn stop_descendants() {
    let me = getpid();
    loop {
        reap(None);
        let below = descendants(me.as_raw_pid());
        if below.is_empty() {
            return;
        }
        for &pid in &below {
            kill_descendant(pid, me.as_raw_pid(), &below);
        }
        thread::sleep(POLL);
    }
}

/// Kill `pid` with SIGKILL through a handle to it, once that handle's process is known to be the one found: its parent
/// is still `root` or one of the processes found `below` it. A process that ended and was reaped after it was found,
/// and whose number another process now has, is left alone.
fn kill_descendant(pid: i32, root: i32, below: &[i32]) {
    let Some(found) = Pid::from_raw(pid) else {
        return;
    };
    let Ok(handle) = pidfd_open(found, PidfdFlags::empty()) else {
        return;
    };
    let parent = fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| parent(&stat));
    if parent.is_some_and(|parent| parent == root || below.contains(&parent)) {
        let _ = pidfd_send_signal(&handle, Signal::KILL);
    }
}

/// Every process below `root`, by the parents that `/proc` lists, zombies included.
fn descendants(root: i32) -> Vec<i32> {
    let parents: Vec<(i32, i32)> = fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let pid: i32 = entry.file_name().to_str()?.parse().ok()?;
            let stat = fs::read_to_string(entry.path().join("stat")).ok()?;
            Some((pid, parent(&stat)?))
        })
        .collect();
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(at) = pending.pop() {
        for &(pid, _) in parents.iter().filter(|(_, parent)| *parent == at) {
            if !found.contains(&pid) {
                found.push(pid);
                pending.push(pid);
            }
        }
    }
    found
}

/// The parent process a `/proc/<pid>/stat` line names: the second field after the command name, whose parentheses may
/// hold anything.
fn parent(stat: &str) -> Option<i32> {
    let (_, rest) = stat.rsplit_once(") ")?;
    rest.split(' ').nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parent_is_read_after_a_command_name_holding_anything() {
        assert_eq!(parent("12 (sh) S 7 12 12 0 -1"), Some(7));
        assert_eq!(parent("12 (a) b) (c) R 9 12"), Some(9));
        assert_eq!(parent("12 (sh) S"), None);
    }
}
