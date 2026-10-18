//! Works out a reusable step's identity, the list of everything its kept result depends on, including its collector's
//! lines and those of every step it needs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::time::Instant;

use camino::Utf8Path;

use crate::report::schedule::{InstanceId, PerInstance, Place};
use crate::report::step::{Launch, RESERVED, launch, shown};
use crate::report::store::{
    Commits, Contribution, Identity, InputIdentity, NeedIdentity, file_sha256,
};
use crate::report::{Prepared, Session, Target, rendered};
use crate::revision::Side;

impl Session<'_> {
    /// Everything the instance's result follows from, given what each instance contributes, or why it cannot be named.
    pub(super) fn identity(
        &self,
        at: InstanceId,
        contributions: &PerInstance<Option<Result<Contribution, String>>>,
    ) -> Result<Identity, String> {
        let instance = self.graph.instance(at);
        let step = self.step(at);
        let dandegate = self.request.executable.clone()?;
        let adapters = self
            .adapter_files
            .clone()
            .map_err(|why| format!("the adapters cannot be listed: {why}"))?;
        let commits = match instance.place {
            Place::Revision(side) => Commits::revision(self.request.revisions[side].commit()),
            Place::Pair => Commits::pair(
                self.request.revisions.before.commit(),
                self.request.revisions.after.commit(),
            ),
        };
        let contributed = |of: InstanceId| match &contributions[of] {
            Some(Ok(contribution)) => Ok(contribution.clone()),
            Some(Err(why)) => Err(why.clone()),
            None => Err("it was not identified".to_owned()),
        };
        let own = contributed(at)?;
        let mut needs = BTreeMap::new();
        let mut used: BTreeSet<_> = step.inputs();
        for need in self.graph.all_needs(at) {
            let needed_instance = self.graph.instance(need);
            let needed = self.step(need);
            used.extend(needed.inputs());
            let Contribution {
                environment,
                collected,
            } = contributed(need).map_err(|why| {
                format!(
                    "{} {}, which it needs, cannot be identified: {why}",
                    needed_instance.place, needed_instance.step
                )
            })?;
            needs.insert(
                format!("{} {}", needed_instance.place, needed_instance.step),
                NeedIdentity {
                    step: needed.written(),
                    environment,
                    collected,
                },
            );
        }
        let root = self.request.repository.root();
        let inputs = used
            .into_iter()
            .map(|name| {
                let value = self.plan().inputs()[name].value().to_owned();
                let path = root.join(&value);
                let file = path
                    .is_file()
                    .then(|| file_sha256(&path).map_err(|error| format!("{path}: {error}")));
                Ok((
                    name.to_string(),
                    InputIdentity {
                        value,
                        file: file.transpose()?,
                    },
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Identity {
            dandegate,
            commits,
            step: step.written(),
            needs,
            inputs,
            adapters,
            environment: own.environment,
            collected: own.collected,
        })
    }

    /// What the instance adds to the identity of each result that follows from it: the variables its step's patterns
    /// match, with the content of the measures file in place of its name, and its collector's lines.
    pub(super) fn contribution(&self, at: InstanceId) -> Result<Contribution, String> {
        let step = self.step(at);
        let overridden = step.env_names();
        let mut environment = BTreeMap::new();
        // A command gets the run's values under the reserved names, never what dandegate inherited under them.
        let inherited = std::env::vars()
            .filter(|(name, _)| !RESERVED.contains(&name.as_str()))
            .chain(self.env.iter().cloned());
        for (name, value) in inherited {
            if overridden.contains(name.as_str())
                || !step
                    .identity_env()
                    .iter()
                    .any(|pattern| pattern.matches(&name))
            {
                continue;
            }
            let value = if name == "DANDEGATE_MEASURES" {
                file_sha256(Utf8Path::new(&value)).map_err(|error| format!("{value}: {error}"))?
            } else {
                value
            };
            environment.insert(name, value);
        }
        Ok(Contribution {
            environment,
            collected: self.collect(at)?,
        })
    }

    /// The `name=value` lines the instance's collector prints in the step's own environment, or none when the step
    /// has no collector.
    fn collect(&self, at: InstanceId) -> Result<BTreeMap<String, String>, String> {
        let instance = self.graph.instance(at);
        let step = self.step(at);
        let out = self.folder(instance).join("out");
        let prepared = match self.target(at) {
            Target::Tree(recipe, side) => match recipe.collect() {
                None => None,
                Some(collect) => Some(self.prepared(recipe, &self.tree(side)?, &out, collect)),
            },
            Target::Pair(recipe) => match recipe.collect() {
                None => None,
                Some(collect) => Some(self.prepared(recipe, &self.pair()?, &out, collect)),
            },
        };
        let Some(Prepared { command, env }) = prepared else {
            return Ok(BTreeMap::new());
        };
        let checkout = match instance.place {
            Place::Revision(side) => self.checkout(side)?,
            Place::Pair => self.checkout(Side::After)?,
        };
        let work = self.work(instance)?;
        fs::create_dir_all(&work).map_err(|error| error.to_string())?;
        let printed = work.join("collected.txt");
        let errors = work.join("collector.log");
        let files = File::create(&printed)
            .and_then(|stdout| File::create(&errors).map(|stderr| (stdout, stderr)))
            .map_err(|error| error.to_string())?;
        let launched = launch(Launch {
            checkout,
            directory: checkout,
            command: &command,
            env: &env,
            stdout: files.0,
            stderr: files.1,
            deadline: Instant::now() + step.timeout(),
        });
        self.log.write(&format!(
            "=== {} {}: identity collector: {}\n{}",
            instance.place,
            instance.step,
            shown(&command),
            fs::read_to_string(&errors).unwrap_or_default()
        ));
        match launched {
            Ok(status) if status.success() => {}
            Ok(status) => return Err(format!("its identity collector failed: {status}")),
            Err(error) => {
                return Err(format!(
                    "its identity collector did not finish: {}",
                    rendered(&error)
                ));
            }
        }
        let text = fs::read_to_string(&printed).map_err(|error| error.to_string())?;
        text.lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                line.split_once('=')
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .ok_or_else(|| {
                        format!("its identity collector printed {line:?}, not name=value")
                    })
            })
            .collect()
    }
}
