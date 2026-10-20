//! Decides where each step runs, what each run waits for, and when each run starts.
//!
//! A step runs as one instance per place. A per-revision step runs on each side, an after step runs on the after side,
//! and a once step runs once for both sides. An instance waits for the instances of the steps it needs, on the sides it
//! uses. [`crate::plan`] says which needs each kind of step can have.
//!
//! The scheduler starts each instance whose needs have finished, up to four at once, each on its own thread. When a
//! need did not succeed, the scheduler skips the instance and records which need failed. An instance on CPUs of its own
//! starts as soon as its needs have finished and is not one of the four, since it takes nothing from them. Instances
//! that run alone, exclusive ones and those whose CPU share did not fit, wait until every other instance has finished,
//! then run one at a time, so a step that measures time runs alone. An instance whose outcome dandegate knows
//! beforehand, because it is disabled, unneeded or reused, takes no thread.

use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;
use std::thread;

use crate::plan::{Kind, Plan, StepName};
use crate::report::step::{Outcome, Skip};
use crate::revision::Side;

/// How many instances run at once.
const PARALLEL: usize = 4;

/// Where an instance runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Place {
    /// On one revision, in its checkout.
    Revision(Side),
    /// Once, given both checkouts.
    Pair,
}

impl Place {
    /// The side a revision's instance runs on, or none for a once step's instance.
    pub(crate) fn side(self) -> Option<Side> {
        match self {
            Self::Revision(side) => Some(side),
            Self::Pair => None,
        }
    }

    /// The place as the output folder names it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Revision(side) => side.as_str(),
            Self::Pair => "both",
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One run of a step at a place.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Instance {
    pub(crate) step: StepName,
    pub(crate) place: Place,
}

/// The places a step of `kind` runs at.
fn places(kind: Kind) -> &'static [Place] {
    match kind {
        Kind::PerRevision => &[Place::Revision(Side::Before), Place::Revision(Side::After)],
        Kind::After => &[Place::Revision(Side::After)],
        Kind::Once => &[Place::Pair],
    }
}

/// An instance of one [`Graph`]: only the graph issues these, so each names one of its instances.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct InstanceId(usize);

/// One instance and the instances it waits for.
#[derive(Debug)]
struct Node {
    instance: Instance,
    needs: Vec<InstanceId>,
}

/// Every step's instances, in the plan's order, each with what it waits for.
#[derive(Debug)]
pub(crate) struct Graph {
    nodes: Vec<Node>,
}

/// One value for each instance of a graph, made by [`Graph::each`].
#[derive(Clone, Debug)]
pub(crate) struct PerInstance<T>(Vec<T>);

impl<T> PerInstance<T> {
    /// Each instance with its value, in the plan's order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (InstanceId, &T)> {
        self.0
            .iter()
            .enumerate()
            .map(|(at, value)| (InstanceId(at), value))
    }
}

impl<T> IntoIterator for PerInstance<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T> std::ops::Index<InstanceId> for PerInstance<T> {
    type Output = T;

    fn index(&self, id: InstanceId) -> &T {
        &self.0[id.0]
    }
}

impl<T> std::ops::IndexMut<InstanceId> for PerInstance<T> {
    fn index_mut(&mut self, id: InstanceId) -> &mut T {
        &mut self.0[id.0]
    }
}

impl Graph {
    pub(crate) fn new(plan: &Plan) -> Self {
        let mut instances = Vec::new();
        for (name, step) in plan.steps() {
            for &place in places(step.kind()) {
                instances.push(Instance {
                    step: name.clone(),
                    place,
                });
            }
        }
        let index = |step: &StepName, place: Place| {
            instances
                .iter()
                .position(|found| found.step == *step && found.place == place)
                .map(InstanceId)
        };
        let needs: Vec<Vec<InstanceId>> = instances
            .iter()
            .map(|instance| {
                let step = &plan.steps()[&instance.step];
                step.needs()
                    .iter()
                    .flat_map(|need| {
                        let needed = &plan.steps()[need];
                        let at: Vec<Place> = match (needed.kind(), instance.place) {
                            (Kind::PerRevision, Place::Revision(side)) => {
                                vec![Place::Revision(side)]
                            }
                            (Kind::PerRevision, Place::Pair) => {
                                vec![Place::Revision(Side::Before), Place::Revision(Side::After)]
                            }
                            (Kind::After, _) => vec![Place::Revision(Side::After)],
                            (Kind::Once, _) => vec![Place::Pair],
                        };
                        at.into_iter()
                            .filter_map(|place| index(need, place))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            })
            .collect();
        let nodes = instances
            .into_iter()
            .zip(needs)
            .map(|(instance, needs)| Node { instance, needs })
            .collect();
        Self { nodes }
    }

    /// A value for each instance, from `value`.
    pub(crate) fn each<T>(&self, value: impl FnMut(InstanceId) -> T) -> PerInstance<T> {
        PerInstance((0..self.nodes.len()).map(InstanceId).map(value).collect())
    }

    /// Every instance, in the plan's order.
    pub(crate) fn ids(&self) -> impl Iterator<Item = InstanceId> + use<> {
        (0..self.nodes.len()).map(InstanceId)
    }

    pub(crate) fn instance(&self, id: InstanceId) -> &Instance {
        &self.nodes[id.0].instance
    }

    /// The instance of `step` at `place`, if the step runs there.
    pub(crate) fn find(&self, step: &StepName, place: Place) -> Option<InstanceId> {
        self.nodes
            .iter()
            .position(|node| node.instance.step == *step && node.instance.place == place)
            .map(InstanceId)
    }

    /// The instances `id` waits for.
    pub(crate) fn needs(&self, id: InstanceId) -> &[InstanceId] {
        &self.nodes[id.0].needs
    }

    /// Every instance `id` waits for, directly or through others.
    pub(crate) fn all_needs(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut found = Vec::new();
        let mut pending = self.needs(id).to_vec();
        while let Some(need) = pending.pop() {
            if !found.contains(&need) {
                found.push(need);
                pending.extend_from_slice(self.needs(need));
            }
        }
        found.sort_unstable();
        found
    }

    /// Every instance, each after those it waits for.
    pub(crate) fn order(&self) -> Vec<InstanceId> {
        fn visit(
            graph: &Graph,
            at: InstanceId,
            done: &mut PerInstance<bool>,
            order: &mut Vec<InstanceId>,
        ) {
            if done[at] {
                return;
            }
            done[at] = true;
            for &need in graph.needs(at) {
                visit(graph, need, done, order);
            }
            order.push(at);
        }
        let mut done = self.each(|_| false);
        let mut order = Vec::with_capacity(self.nodes.len());
        for at in self.ids() {
            visit(self, at, &mut done, &mut order);
        }
        order
    }
}

/// How an instance shares the machine with the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    /// One of up to four at once, on the CPUs no share holds.
    Ordinary,
    /// On CPUs of its own, beside the ordinary instances.
    Own,
    /// Alone, after every other instance.
    Alone,
}

/// How an instance starts.
#[derive(Debug)]
pub(crate) enum Start {
    /// It runs in its lane once its needs succeed.
    Run(Lane),
    /// Its outcome is known already.
    Decided(Outcome),
}

/// Where an instance is in the scheduler.
#[derive(Debug)]
enum State {
    Waiting(Lane),
    Running(Lane),
    Done(Outcome),
}

/// Every instance's outcome, running those that run with `run`.
///
/// # Panics
/// When `run` panics for an instance: once every other running instance has finished, the panic goes on from here.
pub(crate) fn run_all<F>(graph: &Graph, starts: PerInstance<Start>, run: &F) -> PerInstance<Outcome>
where
    F: Fn(InstanceId) -> Outcome + Sync,
{
    let mut states = graph.each(|_| State::Done(Outcome::Disabled));
    for (at, start) in graph.ids().zip(starts) {
        states[at] = match start {
            Start::Run(lane) => State::Waiting(lane),
            Start::Decided(outcome) => State::Done(outcome),
        };
    }
    let succeeded = |state: &State| matches!(state, State::Done(outcome) if outcome.succeeded());
    let done = |state: &State| matches!(state, State::Done(_));
    thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel::<(InstanceId, thread::Result<Outcome>)>();
        loop {
            // Skip what waits for a need that finished without succeeding, until nothing more is skipped.
            loop {
                let skipped: Vec<(InstanceId, StepName)> = states
                    .iter()
                    .filter(|(_, state)| matches!(state, State::Waiting(_)))
                    .filter_map(|(at, _)| {
                        let failed = graph
                            .needs(at)
                            .iter()
                            .find(|&&need| done(&states[need]) && !succeeded(&states[need]))?;
                        Some((at, graph.instance(*failed).step.clone()))
                    })
                    .collect();
                if skipped.is_empty() {
                    break;
                }
                for (at, need) in skipped {
                    states[at] = State::Done(Outcome::Skipped(Skip::NeedFailed(need)));
                }
            }
            let others_left = states
                .iter()
                .any(|(_, state)| matches!(state, State::Waiting(lane) if *lane != Lane::Alone));
            let running = states
                .iter()
                .filter(|(_, state)| matches!(state, State::Running(_)))
                .count();
            let ordinary = states
                .iter()
                .filter(|(_, state)| matches!(state, State::Running(Lane::Ordinary)))
                .count();
            let mut starting: Vec<(InstanceId, Lane)> = Vec::new();
            for (at, state) in states.iter() {
                let State::Waiting(lane) = *state else {
                    continue;
                };
                let room = match lane {
                    Lane::Alone => !others_left && running == 0 && starting.is_empty(),
                    Lane::Own => true,
                    Lane::Ordinary => {
                        let starting_ordinary = starting
                            .iter()
                            .filter(|(_, lane)| *lane == Lane::Ordinary)
                            .count();
                        ordinary + starting_ordinary < PARALLEL
                    }
                };
                if room && graph.needs(at).iter().all(|&need| done(&states[need])) {
                    starting.push((at, lane));
                }
            }
            let started = !starting.is_empty();
            for (at, lane) in starting {
                states[at] = State::Running(lane);
                let sender = sender.clone();
                scope.spawn(move || {
                    // A panic is sent too, so the scheduler never waits for an outcome that cannot come.
                    let outcome = panic::catch_unwind(AssertUnwindSafe(|| run(at)));
                    // The receiver lives until every instance has reported, so the send cannot fail.
                    let _ = sender.send((at, outcome));
                });
            }
            if running == 0 && !started {
                break;
            }
            match receiver.recv() {
                Ok((at, Ok(outcome))) => {
                    states[at] = State::Done(outcome);
                }
                // The scope waits for the other running instances, then the panic goes on.
                Ok((_, Err(payload))) => panic::resume_unwind(payload),
                Err(_) => break,
            }
        }
    });
    graph.each(|at| {
        match std::mem::replace(&mut states[at], State::Done(Outcome::Disabled)) {
            State::Done(outcome) => outcome,
            // Every waiting instance's needs finish, so the loop ends with none waiting or running.
            State::Waiting(_) | State::Running(_) => Outcome::Skipped(Skip::Unneeded),
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use camino::Utf8Path;

    use super::*;
    use crate::plan::Tree;

    fn graph(text: &str) -> Graph {
        let plan = Plan::read(
            text.as_bytes(),
            Utf8Path::new("dandegate.toml"),
            &[],
            &[],
            &Tree::default(),
        )
        .expect("a plan");
        Graph::new(&plan)
    }

    #[test]
    fn a_panic_in_one_instance_goes_on_once_the_others_have_finished() {
        let graph = graph(
            "[[steps.step]]\nname = \"bad\"\nruns = \"once\"\nrun = [\"x\"]\n\n\
             [[steps.step]]\nname = \"good\"\nruns = \"once\"\nrun = [\"x\"]\n",
        );
        let bad = graph.ids().next();
        let finished = AtomicBool::new(false);
        let caught = panic::catch_unwind(AssertUnwindSafe(|| {
            run_all(&graph, graph.each(|_| Start::Run(Lane::Ordinary)), &|at| {
                assert_ne!(Some(at), bad, "a bug in one step's work");
                finished.store(true, Ordering::SeqCst);
                Outcome::Disabled
            })
        }));
        let payload = caught.expect_err("the panic goes on");
        let message = payload
            .downcast_ref::<String>()
            .expect("a formatted message");
        assert!(message.contains("a bug in one step's work"), "{message}");
        assert!(
            finished.load(Ordering::SeqCst),
            "the other instance finished"
        );
    }
}
