//! The CPUs each step runs on. A step with a CPU share, `cpus = <count>` in the steps file, runs on CPUs of its own
//! while the other steps run on the rest, instead of running alone after them.
//!
//! dandegate counts the CPUs it may run on, its affinity, as N. The enabled steps with a share ask for S CPUs together,
//! counting each instance. When the rest keeps at least as many as the shares take, 2S at most N, each such instance
//! gets its count of CPUs from the highest-numbered down, in the plan's order, and every other instance gets the rest.
//! Otherwise there are too few CPUs to split, and each step with a share runs alone, as an exclusive step does, on all
//! N. The assignment is made once, before any identity is collected, from the enabled steps whether or not they run,
//! so collection, `{cpus}`, execution, kept results' identities and the page all see the same CPUs. A step's
//! supervisor sets its CPUs before it starts the command, so everything the command starts runs on them, and `{cpus}`
//! names them, as a comma-separated list that `taskset -c` takes, for a program that pins its own work.

use std::fmt;

use rustix::thread::{CpuSet, sched_getaffinity, sched_setaffinity};

use crate::report::schedule::{Graph, InstanceId, Lane, PerInstance};

/// A set of CPUs by number, in increasing order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cpus(Vec<usize>);

impl Cpus {
    /// The CPUs this thread may run on; where the kernel cannot say, as many as the standard library counts, from 0.
    pub(crate) fn allowed() -> Self {
        match sched_getaffinity(None) {
            Ok(set) => Self(
                (0..CpuSet::MAX_CPU)
                    .filter(|&cpu| set.is_set(cpu))
                    .collect(),
            ),
            Err(_) => {
                Self((0..std::thread::available_parallelism().map_or(1, usize::from)).collect())
            }
        }
    }

    /// Run this thread, and so every process it starts, on these CPUs only.
    ///
    /// # Errors
    /// The kernel's error, or one for a CPU beyond what the kernel's set can hold.
    pub(crate) fn enter(&self) -> std::io::Result<()> {
        let mut set = CpuSet::new();
        for &cpu in &self.0 {
            if cpu >= CpuSet::MAX_CPU {
                return Err(std::io::Error::other(format!(
                    "CPU {cpu} is beyond the kernel's set"
                )));
            }
            set.set(cpu);
        }
        sched_setaffinity(None, &set)?;
        Ok(())
    }

    /// The CPUs a comma-separated list names, or none for one that is malformed or empty.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let cpus = text
            .split(',')
            .map(|cpu| cpu.parse().ok())
            .collect::<Option<Vec<usize>>>()?;
        (!cpus.is_empty()).then_some(Self(cpus))
    }
}

/// The list as `taskset -c` takes it: CPU numbers joined by commas.
impl fmt::Display for Cpus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let written: Vec<String> = self.0.iter().map(ToString::to_string).collect();
        formatter.write_str(&written.join(","))
    }
}

/// Where one instance runs.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Assigned {
    /// On dandegate's own CPUs, unenforced: no step shares them out, or the instance is not enabled.
    All,
    /// On these CPUs, which the shares leave, or on all of them when the shares run alone.
    Rest(Cpus),
    /// On these CPUs, its share, beside the other instances.
    Own(Cpus),
    /// On all of them, alone after every other instance: its share did not fit.
    Alone,
}

/// The CPUs each instance runs on, decided once for the whole report.
#[derive(Debug)]
pub(crate) struct Allocation {
    allowed: Cpus,
    assigned: PerInstance<Assigned>,
}

impl Allocation {
    /// Share `allowed` among the enabled instances, `share` giving each one's CPU share, if it has one.
    pub(crate) fn new(
        graph: &Graph,
        allowed: Cpus,
        share: impl Fn(InstanceId) -> Option<Option<usize>>,
    ) -> Self {
        let shares: Vec<(InstanceId, usize)> = graph
            .ids()
            .filter_map(|at| share(at).flatten().map(|count| (at, count)))
            .collect();
        let enabled = |at| share(at).is_some();
        let mut assigned = graph.each(|_| Assigned::All);
        if !shares.is_empty() {
            let counts: Vec<usize> = shares.iter().map(|&(_, count)| count).collect();
            match split(&allowed, &counts) {
                Split::Shared { own, rest } => {
                    for at in graph.ids().filter(|&at| enabled(at)) {
                        assigned[at] = Assigned::Rest(rest.clone());
                    }
                    for (&(at, _), cpus) in shares.iter().zip(own) {
                        assigned[at] = Assigned::Own(cpus);
                    }
                }
                Split::Alone => {
                    for at in graph.ids().filter(|&at| enabled(at)) {
                        assigned[at] = Assigned::Rest(allowed.clone());
                    }
                    for &(at, _) in &shares {
                        assigned[at] = Assigned::Alone;
                    }
                }
            }
        }
        Self { allowed, assigned }
    }

    /// The CPUs the instance's commands are held to, or none when they keep dandegate's own.
    pub(crate) fn enforced(&self, at: InstanceId) -> Option<&Cpus> {
        match &self.assigned[at] {
            Assigned::Rest(cpus) | Assigned::Own(cpus) => Some(cpus),
            Assigned::All | Assigned::Alone => None,
        }
    }

    /// The CPUs the instance runs on, as `{cpus}` names them.
    pub(crate) fn cpus(&self, at: InstanceId) -> &Cpus {
        self.enforced(at).unwrap_or(&self.allowed)
    }

    /// How the instance shares the machine, an exclusive step running alone whatever its CPUs.
    pub(crate) fn lane(&self, at: InstanceId, exclusive: bool) -> Lane {
        match &self.assigned[at] {
            Assigned::Own(_) => Lane::Own,
            Assigned::Alone => Lane::Alone,
            Assigned::All | Assigned::Rest(_) if exclusive => Lane::Alone,
            Assigned::All | Assigned::Rest(_) => Lane::Ordinary,
        }
    }

    /// The instance's CPUs as the page shows them, when the report shares them out.
    pub(crate) fn label(&self, at: InstanceId) -> Option<String> {
        match &self.assigned[at] {
            Assigned::All => None,
            Assigned::Rest(cpus) => Some(cpus.to_string()),
            Assigned::Own(cpus) => Some(format!("{cpus}, its own")),
            Assigned::Alone => Some(format!("{}, alone: too few to share", self.allowed)),
        }
    }

    /// How the CPUs were shared out, for the report's log, when they were.
    pub(crate) fn summary(&self) -> Option<String> {
        let rest = self
            .assigned
            .iter()
            .find_map(|(_, assigned)| match assigned {
                Assigned::Rest(cpus) => Some(cpus),
                Assigned::All | Assigned::Own(_) | Assigned::Alone => None,
            });
        let alone = self
            .assigned
            .iter()
            .any(|(_, assigned)| *assigned == Assigned::Alone);
        match (rest, alone) {
            (_, true) => Some(format!(
                "the shares ask for more than half of the CPUs dandegate may use, {}, so each step with a share runs \
                 alone",
                self.allowed
            )),
            (Some(rest), false) => Some(format!(
                "dandegate may use {}; the steps with a share run on their own, the others on {rest}",
                self.allowed
            )),
            (None, false) => None,
        }
    }
}

/// How `allowed` is shared out.
#[derive(Debug, PartialEq, Eq)]
enum Split {
    /// Each share's own CPUs, in the order the shares were given, and the rest, for every other step.
    Shared { own: Vec<Cpus>, rest: Cpus },
    /// Too few CPUs to split: each step with a share runs alone, on every CPU.
    Alone,
}

/// Share `allowed` among `shares`, each a count of CPUs, as the module describes.
fn split(allowed: &Cpus, shares: &[usize]) -> Split {
    let total: usize = shares.iter().sum();
    if total == 0 || total.saturating_mul(2) > allowed.0.len() {
        return Split::Alone;
    }
    let mut free = allowed.0.clone();
    let own = shares
        .iter()
        .map(|&count| {
            let mut taken = free.split_off(free.len() - count);
            taken.sort_unstable();
            Cpus(taken)
        })
        .collect();
    Split::Shared {
        own,
        rest: Cpus(free),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpus(numbers: &[usize]) -> Cpus {
        Cpus(numbers.to_vec())
    }

    #[test]
    fn a_list_is_written_and_read_as_numbers_joined_by_commas() {
        assert_eq!(cpus(&[0, 1, 8]).to_string(), "0,1,8");
        assert_eq!(Cpus::parse("0,1,8"), Some(cpus(&[0, 1, 8])));
        for malformed in ["", "a", "1,", ",1", "0-3"] {
            assert_eq!(Cpus::parse(malformed), None, "{malformed:?}");
        }
    }

    #[test]
    fn shares_take_the_highest_cpus_in_order_and_the_rest_keeps_at_least_as_many() {
        let range = |count: usize| cpus(&(0..count).collect::<Vec<_>>());
        assert_eq!(
            split(&range(16), &[4]),
            Split::Shared {
                own: vec![cpus(&[12, 13, 14, 15])],
                rest: range(12),
            }
        );
        assert_eq!(
            split(&cpus(&[0, 1, 2, 3, 8, 9, 10, 11]), &[2, 1]),
            Split::Shared {
                own: vec![cpus(&[10, 11]), cpus(&[9])],
                rest: cpus(&[0, 1, 2, 3, 8]),
            }
        );
        // Four CPUs hold a share of two beside the other steps' two, not a share of three.
        assert_eq!(
            split(&range(4), &[2]),
            Split::Shared {
                own: vec![cpus(&[2, 3])],
                rest: range(2),
            }
        );
        assert_eq!(split(&range(4), &[3]), Split::Alone);
        assert_eq!(split(&range(6), &[2, 2]), Split::Alone);
        assert_eq!(split(&range(1), &[1]), Split::Alone);
    }
}
