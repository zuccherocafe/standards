//! The shards, run at once, each cargo-mutants in a session of its own, so stopping a shard's process group stops
//! everything it started and a terminal's interrupt reaches only this process.
//!
//! [`Running`] owns the started shards. Dropped before every shard finished, after a failure or an interrupt, it stops
//! each one still running and waits for it, so nothing a run started outlives it. [`Copies`] removes the shards'
//! copies of the repository when dropped; declared before [`Running`], it is dropped after it, once every shard has
//! stopped.

use std::fs;
use std::io;
use std::num::NonZeroU32;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process_group};

use crate::command::{Shard, Work};

/// How often a wait looks for a shard's end and for an interrupt.
const POLL: Duration = Duration::from_millis(20);

/// The started shards.
#[derive(Debug, Default)]
pub(crate) struct Running {
    children: Vec<(Shard, Child)>,
}

impl Running {
    /// Starts `command` as `shard`, in a new session.
    ///
    /// # Errors
    /// The error starting the command's program.
    pub(crate) fn start(&mut self, shard: Shard, mut command: Command) -> io::Result<()> {
        let child = command.setsid(true).spawn()?;
        self.children.push((shard, child));
        Ok(())
    }

    /// Each shard's exit code, in shard order, once every shard has ended: the code it exited with, or the negated
    /// number of the signal that ended it. The shards all run while the wait takes them in turn.
    ///
    /// # Errors
    /// [`WaitError::Interrupted`] when `interrupted` is set before every shard is seen to end, even when they all
    /// have, and [`WaitError::Wait`] when a shard's state cannot be read; the shards are then stopped as this value
    /// drops.
    pub(crate) fn wait(mut self, interrupted: &AtomicBool) -> Result<Vec<(Shard, i32)>, WaitError> {
        let mut codes = Vec::with_capacity(self.children.len());
        for (shard, child) in &mut self.children {
            let status = loop {
                let ended = child.try_wait().map_err(WaitError::Wait)?;
                if interrupted.load(Ordering::SeqCst) {
                    return Err(Interrupted.into());
                }
                if let Some(status) = ended {
                    break status;
                }
                thread::sleep(POLL);
            };
            codes.push((*shard, code(status)));
        }
        Ok(codes)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        for (_, child) in &mut self.children {
            if matches!(child.try_wait(), Ok(None)) {
                // The child has not been reaped, so its number, which is its session's and group's, is still its own.
                if let Some(group) = Pid::from_raw(child.id().cast_signed()) {
                    let _ = kill_process_group(group, Signal::TERM);
                }
            }
        }
        for (_, child) in &mut self.children {
            let _ = child.wait();
        }
    }
}

/// The exit code of a shard that ended with `status`, as a shell would print it negated for a signal.
fn code(status: ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| -signal))
        .expect("an ended process exited or was ended by a signal")
}

/// The copies of the repository a run of `count` shards makes, removed when dropped.
#[derive(Debug)]
pub(crate) struct Copies<'w> {
    pub(crate) work: &'w Work,
    pub(crate) count: NonZeroU32,
}

impl Drop for Copies<'_> {
    fn drop(&mut self) {
        for shard in Shard::all(self.count) {
            let _ = fs::remove_dir_all(self.work.copy(shard));
        }
    }
}

/// The run was interrupted before every shard finished.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("interrupted")]
pub(crate) struct Interrupted;

/// Why the wait for the shards ended before every shard was seen to end.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WaitError {
    #[error(transparent)]
    Interrupted(#[from] Interrupted),
    #[error("could not wait for a shard")]
    Wait(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shard_ended_by_a_signal_has_the_signals_number_negated_as_its_code() {
        assert_eq!(code(ExitStatus::from_raw(4 << 8)), 4);
        assert_eq!(code(ExitStatus::from_raw(9)), -9);
    }
}
