//! Runs one report, from the checkouts to the page, after dandegate has accepted the command line, the revisions and
//! the plan. Nothing in this module stops the page. A step that fails, a checkout that dandegate cannot make, and a
//! store that it cannot write each become an outcome or a note on the page.
//!
//! A report runs in this order:
//!
//! 1. Check out both commits as detached worktrees in a folder of `.cache/dandegate/scratch/` that this run makes for
//!    itself.
//! 2. For each reusable step, work out its identity, running its collector, and look for a kept result.
//! 3. Decide which steps run. A step that only other steps need runs only when one of them runs fresh.
//! 4. Run the steps, and keep each complete success.
//! 5. Remove the checkouts.
//! 6. Compare the two sides, and write the page, `measures.json` and the printed lines.
//!
//! [`schedule`] orders the steps and runs up to four at once. [`step`] runs one command and records what happened to a
//! step. [`store`] keeps complete successes. [`identity`] lists what a kept result depends on. [`summary`] compares the
//! two sides and writes the page.

mod identity;
mod schedule;
pub(crate) mod step;
pub(crate) mod store;
mod summary;

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};

use crate::compare::measure::Measures;
use crate::plan::{
    Arg, Expansion, PairArg, PairValues, Plan, Recipe, Report, Runs, Step, Template, TreeArg,
    TreeValues, Work,
};
use crate::report::schedule::{Graph, Instance, InstanceId, PerInstance, Place, Start, run_all};
use crate::report::step::{Launch, LaunchError, Outcome, Skip, check_outputs, launch, shown};
use crate::report::store::{Identity, Lookup, Store};
use crate::revision::git::Repository;
use crate::revision::{PerSide, Revision, Side};
use crate::rust;

/// Everything a report needs, accepted and resolved.
#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) repository: Repository,
    pub(crate) revisions: PerSide<Revision>,
    pub(crate) plan: Plan,
    /// The new output folder, already made.
    pub(crate) out: Utf8PathBuf,
    pub(crate) no_reuse: bool,
    /// `--measures`' file, absolute.
    pub(crate) measures_file: Option<Utf8PathBuf>,
    /// The measures `--previous` and `--measures` give each side.
    pub(crate) imported: PerSide<Measures>,
    /// dandegate's own executable's digest, or why it cannot be read.
    pub(crate) executable: Result<String, String>,
}

/// The report's log, written by every step's thread in turn.
#[derive(Debug)]
struct Log(Mutex<Option<File>>);

impl Log {
    fn write(&self, text: &str) {
        if let Ok(mut file) = self.0.lock()
            && let Some(file) = file.as_mut()
        {
            // The log is evidence beside the page. If a write to it fails, the page is still written.
            let _ = file.write_all(text.as_bytes());
        }
    }
}

/// What the run shares between its steps' threads.
struct Session<'a> {
    request: &'a Request,
    graph: Graph,
    checkouts: PerSide<Result<Utf8PathBuf, String>>,
    /// The scratch folder this run made for itself, or why it could not.
    scratch: Result<Utf8PathBuf, String>,
    adapters: Utf8PathBuf,
    adapter_files: Result<BTreeMap<String, String>, String>,
    env: Vec<(String, String)>,
    log: Log,
    store: Store,
    identities: PerInstance<Option<Result<Identity, String>>>,
    notes: Mutex<BTreeMap<InstanceId, Vec<String>>>,
}

/// What a report printed, and whether its page was written.
#[derive(Debug)]
pub(crate) struct Finished {
    pub(crate) lines: Vec<String>,
    pub(crate) written: bool,
}

/// Write the report into `request.out`, and give the lines to print.
pub(crate) fn report(request: &Request) -> Finished {
    let root = request.repository.root();
    let cache = root.join(".cache/dandegate");
    let log = Log(Mutex::new(
        File::create(request.out.join("report.log")).ok(),
    ));
    log.write(&format!(
        "dandegate: before {}, after {}\nrepository {root}\n",
        request.revisions.before, request.revisions.after
    ));
    let scratch = prepare_cache(&cache);
    let checkouts = PerSide::from_fn(|side| {
        let folder = scratch.as_ref().map_err(Clone::clone)?.join(side.as_str());
        request
            .repository
            .add_worktree(&folder, request.revisions[side].commit())
            .map_err(|error| error.to_string())
            .map(|()| folder)
    });
    let adapters = checkouts
        .after
        .as_ref()
        .map_or_else(|_| cache.join("scratch"), Clone::clone)
        .join("dandegate");
    let mut env = vec![("DANDEGATE_REPOSITORY".to_owned(), root.to_string())];
    if let Some(file) = &request.measures_file {
        env.push(("DANDEGATE_MEASURES".to_owned(), file.to_string()));
    }
    let graph = Graph::new(&request.plan);
    let identities = graph.each(|_| None);
    let mut session = Session {
        request,
        graph,
        checkouts,
        adapter_files: request
            .repository
            .blobs(request.revisions.after.commit(), "dandegate")
            .map_err(|error| error.to_string()),
        scratch,
        adapters,
        env,
        log,
        store: Store::new(cache.join("results")),
        identities,
        notes: Mutex::new(BTreeMap::new()),
    };
    let starts = session.starts();
    let outcomes = run_all(&session.graph, starts, &|at| session.execute(at));
    session.remove_checkouts();
    session.page(&outcomes)
}

/// Make the cache folder, ignored by git whatever the project ignores, and a new scratch folder for this run.
fn prepare_cache(cache: &Utf8Path) -> Result<Utf8PathBuf, String> {
    let made = (|| -> io::Result<Utf8PathBuf> {
        fs::create_dir_all(cache)?;
        let ignore = cache.join(".gitignore");
        if !ignore.exists() {
            fs::write(ignore, "*\n")?;
        }
        acquire(&cache.join("scratch"), "")
    })();
    made.map_err(|error| format!("no scratch folder could be made under {cache}: {error}"))
}

/// A new folder in `parent`, named `prefix`, this process's number and a counter, made by this call and so owned by
/// it. A process in another PID namespace can have the same number, so a name that exists is passed over, never
/// reused or removed.
fn acquire(parent: &Utf8Path, prefix: &str) -> io::Result<Utf8PathBuf> {
    static TAKEN: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(parent)?;
    loop {
        let number = TAKEN.fetch_add(1, Ordering::Relaxed);
        let folder = parent.join(format!("{prefix}{}-{number}", std::process::id()));
        match fs::create_dir(&folder) {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            made => return made.map(|()| folder),
        }
    }
}

/// What an instance runs: a per-revision or after step's recipe on one side, or a once step's on both.
enum Target<'a> {
    Tree(&'a Recipe<TreeArg>, Side),
    Pair(&'a Recipe<PairArg>),
}

/// A command with its placeholders filled in, and the environment it runs with.
struct Prepared {
    command: Vec<String>,
    env: Vec<(String, String)>,
}

/// Expand every template.
fn expanded<A: Arg>(templates: &[Template<A>], expansion: &Expansion<'_, '_, A>) -> Vec<String> {
    templates
        .iter()
        .map(|template| template.expand(expansion))
        .collect()
}

impl Session<'_> {
    fn plan(&self) -> &Plan {
        &self.request.plan
    }

    fn step(&self, at: InstanceId) -> &Step {
        &self.plan().steps()[&self.graph.instance(at).step]
    }

    /// What an instance runs, with the side it runs on.
    fn target(&self, at: InstanceId) -> Target<'_> {
        let place = self.graph.instance(at).place;
        match self.step(at).runs() {
            Runs::PerRevision(recipe) | Runs::After(recipe) => Target::Tree(
                recipe,
                place
                    .side()
                    .expect("a per-revision or after step's instances each run on a revision"),
            ),
            Runs::Once(recipe) => Target::Pair(recipe),
        }
    }

    /// An instance's folder in the output: its log and its output folder, `out/`.
    fn folder(&self, instance: &Instance) -> Utf8PathBuf {
        self.request
            .out
            .join("steps")
            .join(instance.place.as_str())
            .join(instance.step.as_str())
    }

    /// An instance's own scratch folder.
    fn work(&self, instance: &Instance) -> Result<Utf8PathBuf, String> {
        Ok(self
            .scratch
            .as_ref()
            .map_err(Clone::clone)?
            .join("work")
            .join(format!(
                "{}-{}",
                instance.place.as_str(),
                instance.step.as_str()
            )))
    }

    fn note(&self, at: InstanceId, text: String) {
        let instance = self.graph.instance(at);
        self.log.write(&format!(
            "=== {} {}: {text}\n",
            instance.place, instance.step
        ));
        if let Ok(mut notes) = self.notes.lock() {
            notes.entry(at).or_default().push(text);
        }
    }

    fn checkout(&self, side: Side) -> Result<&Utf8Path, String> {
        self.checkouts[side]
            .as_deref()
            .map_err(|why| format!("the {} checkout could not be made: {why}", side.as_str()))
    }

    fn tree(&self, side: Side) -> Result<TreeValues<'_>, String> {
        Ok(TreeValues {
            tree: self.checkout(side)?,
            commit: self.request.revisions[side].commit(),
        })
    }

    fn pair(&self) -> Result<PairValues<'_>, String> {
        Ok(PairValues {
            before: self.tree(Side::Before)?,
            after: self.tree(Side::After)?,
        })
    }

    /// How each instance starts: decided, reused, or run.
    fn starts(&mut self) -> PerInstance<Start> {
        let mut forced = self.graph.each(|_| false);
        for at in self.graph.ids() {
            let step = self.step(at);
            if step.enabled() && step.live() {
                for &need in self.graph.needs(at) {
                    forced[need] = true;
                }
            }
        }
        let reusable = |at: InstanceId| self.step(at).enabled() && self.step(at).reuse();
        // Every reusable instance, and every instance one needs, adds to identities.
        let mut identified = self.graph.each(reusable);
        for at in self.graph.ids().filter(|&at| reusable(at)) {
            for need in self.graph.all_needs(at) {
                identified[need] = true;
            }
        }
        let contributions = self
            .graph
            .each(|at| identified[at].then(|| self.contribution(at)));
        let identities = self
            .graph
            .each(|at| reusable(at).then(|| self.identity(at, &contributions)));
        let mut found: PerInstance<Option<Utf8PathBuf>> = self.graph.each(|_| None);
        for (at, identity) in identities.iter() {
            let Some(identity) = identity else {
                continue;
            };
            let why = match identity {
                Err(why) => format!("runs fresh and is not kept: {why}"),
                Ok(_) if self.request.no_reuse => "runs fresh: --no-reuse".to_owned(),
                Ok(_) if forced[at] => {
                    "runs fresh: a live step needs its checkout from this run".to_owned()
                }
                Ok(identity) => match self.store.find(identity) {
                    Lookup::Hit(entry) => {
                        found[at] = Some(entry);
                        continue;
                    }
                    Lookup::Miss(why) => format!("runs fresh: {why}"),
                },
            };
            self.note(at, why);
        }
        self.identities = identities;

        let mut needed_by: PerInstance<Vec<InstanceId>> = self.graph.each(|_| Vec::new());
        for at in self.graph.ids() {
            if self.step(at).enabled() {
                for &need in self.graph.needs(at) {
                    needed_by[need].push(at);
                }
            }
        }
        let mut wanted = self.graph.each(|_| false);
        for at in self.graph.order().into_iter().rev() {
            wanted[at] = self.step(at).enabled()
                && (needed_by[at].is_empty()
                    || needed_by[at]
                        .iter()
                        .any(|&by| wanted[by] && found[by].is_none()));
        }

        let rust_changed = self.plan().rust().map(|settings| {
            rust::rust_changed(&self.request.repository, settings, &self.request.revisions)
                .unwrap_or(true)
        });
        self.graph.each(|at| {
            let step = self.step(at);
            if !step.enabled() {
                return Start::Decided(Outcome::Disabled);
            }
            if !wanted[at] {
                return Start::Decided(Outcome::Skipped(Skip::Unneeded));
            }
            if step.report() == Some(Report::Mutants) && rust_changed == Some(false) {
                return Start::Decided(Outcome::Skipped(Skip::NoRustChanged));
            }
            if let Some(entry) = &found[at] {
                let folder = self.folder(self.graph.instance(at));
                match Store::restore(entry, &folder) {
                    Ok(()) => {
                        self.note(at, format!("reused {entry}"));
                        let outputs = check_outputs(step.outputs(), &folder.join("out"));
                        return Start::Decided(Outcome::Reused {
                            entry: entry.clone(),
                            outputs,
                        });
                    }
                    Err(error) => {
                        let why =
                            format!("runs fresh: the kept result could not be copied: {error}");
                        self.note(at, why);
                    }
                }
            }
            Start::Run {
                exclusive: step.exclusive(),
            }
        })
    }

    /// `command` with its placeholders filled in, and the recipe's environment.
    fn prepared<A: Arg>(
        &self,
        recipe: &Recipe<A>,
        values: &A::Values<'_>,
        out: &Utf8Path,
        command: &[Template<A>],
    ) -> Prepared {
        let expansion = Expansion {
            values,
            inputs: self.plan().inputs(),
            out,
            adapters: &self.adapters,
        };
        let command = expanded(command, &expansion);
        let mut env = self.env.clone();
        env.extend(
            recipe
                .env()
                .iter()
                .map(|(name, value)| (name.as_str().to_owned(), value.expand(&expansion))),
        );
        Prepared { command, env }
    }

    /// Run one instance and keep its result when it is a complete success.
    fn execute(&self, at: InstanceId) -> Outcome {
        let instance = self.graph.instance(at);
        self.log.write(&format!(
            "=== {} {}: started\n",
            instance.place, instance.step
        ));
        let folder = self.folder(instance);
        let out = folder.join("out");
        let log_path = folder.join("log.txt");
        let outcome = match fs::create_dir_all(&out).and_then(|()| File::create(&log_path)) {
            Err(error) => {
                Outcome::LaunchFailed(format!("its folder {folder} could not be made: {error}"))
            }
            Ok(_) => match self.target(at) {
                Target::Tree(recipe, side) => match self.tree(side) {
                    Err(why) => Outcome::LaunchFailed(why),
                    Ok(values) => self.perform(at, recipe, &values, Some(side), &out, &log_path),
                },
                Target::Pair(recipe) => match self.pair() {
                    Err(why) => Outcome::LaunchFailed(why),
                    Ok(values) => self.perform(at, recipe, &values, None, &out, &log_path),
                },
            },
        };
        if let Ok(mut log) = OpenOptions::new().append(true).open(&log_path) {
            let _ = writeln!(log, "--- {outcome}");
        }
        self.log.write(&format!(
            "=== {} {}: {outcome}\n{}",
            instance.place,
            instance.step,
            fs::read_to_string(&log_path).unwrap_or_default()
        ));
        if let Some(Ok(identity)) = &self.identities[at]
            && outcome.complete()
        {
            match self.store.keep(identity, &folder) {
                Ok(()) => self.note(at, "kept for reuse".to_owned()),
                Err(error) => self.note(at, format!("not kept: {}", rendered(&error))),
            }
        } else if let Some(Ok(_)) = &self.identities[at] {
            self.note(at, "not kept: only a complete success is kept".to_owned());
        }
        outcome
    }

    /// Run an instance's command or report.
    fn perform<A: Arg>(
        &self,
        at: InstanceId,
        recipe: &Recipe<A>,
        values: &A::Values<'_>,
        side: Option<Side>,
        out: &Utf8Path,
        log_path: &Utf8Path,
    ) -> Outcome {
        let instance = self.graph.instance(at);
        let step = self.step(at);
        let deadline = Instant::now() + step.timeout();
        let checkout_side = side.unwrap_or(Side::After);
        let checkout = match self.checkout(checkout_side) {
            Ok(checkout) => checkout,
            Err(why) => return Outcome::LaunchFailed(why),
        };
        match recipe.work() {
            Work::Command(run) => {
                let Prepared { command, env } = self.prepared(recipe, values, out, run);
                let opened = OpenOptions::new()
                    .append(true)
                    .open(log_path)
                    .and_then(|mut log| {
                        writeln!(log, "$ {}", shown(&command))?;
                        for (name, value) in env.iter().skip(self.env.len()) {
                            writeln!(log, "  {name}={value}")?;
                        }
                        let stderr = log.try_clone()?;
                        Ok((log, stderr))
                    });
                let (stdout, stderr) = match opened {
                    Ok(files) => files,
                    Err(error) => {
                        return Outcome::LaunchFailed(format!(
                            "its log could not be written: {error}"
                        ));
                    }
                };
                let launched = launch(Launch {
                    checkout,
                    directory: checkout,
                    command: &command,
                    env: &env,
                    stdout,
                    stderr,
                    deadline,
                });
                match launched {
                    Ok(status) => Outcome::Exited {
                        status,
                        outputs: check_outputs(step.outputs(), out),
                    },
                    Err(LaunchError::TimedOut) => Outcome::TimedOut {
                        after: step.timeout(),
                    },
                    Err(error) => Outcome::LaunchFailed(rendered(&error)),
                }
            }
            Work::Rust(report) => {
                let Some(settings) = self.plan().rust() else {
                    return Outcome::LaunchFailed("the plan has no Rust workspace".to_owned());
                };
                let listings = PerSide::from_fn(|side| {
                    self.folder(&Instance {
                        step: Report::ApiListings.step(),
                        place: Place::Revision(side),
                    })
                    .join("out")
                });
                let work = match self.work(instance) {
                    Ok(work) => work,
                    Err(why) => return Outcome::LaunchFailed(why),
                };
                if let Err(error) = fs::create_dir_all(&work) {
                    return Outcome::LaunchFailed(format!(
                        "its scratch folder could not be made: {error}"
                    ));
                }
                let context = rust::Context {
                    settings,
                    revisions: &self.request.revisions,
                    checkouts: &self.checkouts,
                    out,
                    listings,
                    scratch: &work,
                    log: log_path,
                    env: &self.env,
                    deadline,
                };
                match rust::run(*report, side, &context) {
                    Ok(()) => Outcome::Reported {
                        outputs: check_outputs(step.outputs(), out),
                    },
                    Err(rust::Stop::TimedOut) => Outcome::TimedOut {
                        after: step.timeout(),
                    },
                    Err(error @ rust::Stop::Write(_)) => Outcome::LaunchFailed(rendered(&error)),
                }
            }
        }
    }

    fn remove_checkouts(&self) {
        for side in Side::BOTH {
            if let Ok(folder) = &self.checkouts[side]
                && let Err(error) = self.request.repository.remove_worktree(folder)
            {
                self.log.write(&format!(
                    "=== the {} checkout could not be removed: {error}\n",
                    side.as_str()
                ));
                let _ = fs::remove_dir_all(folder);
            }
        }
        let _ = self.request.repository.prune_worktrees();
        // Only a folder this run made is removed.
        if let Ok(scratch) = &self.scratch {
            let _ = fs::remove_dir_all(scratch);
        }
    }
}

/// An error and each of its causes, once, on one line.
pub(crate) fn rendered(error: &dyn std::error::Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut cause = error.source();
    while let Some(found) = cause {
        parts.push(found.to_string());
        cause = found.source();
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_acquired_new_and_a_name_that_cannot_be_made_is_an_error_not_a_retry() {
        let temporary =
            Utf8PathBuf::try_from(std::env::temp_dir()).expect("a UTF-8 temporary folder");
        let parent = acquire(&temporary, "dandegate-acquire-").expect("a folder of its own");
        let first = acquire(&parent, "run-").expect("a folder");
        let second = acquire(&parent, "run-").expect("another folder");
        assert_ne!(first, second);
        assert!(first.is_dir() && second.is_dir());
        // A name longer than a file system allows fails the same way however often it is tried.
        assert!(acquire(&parent, &"x".repeat(300)).is_err());
        let _ = fs::remove_dir_all(&parent);
    }
}
