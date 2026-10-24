//! Run authored mutation cases in isolated Cargo workspaces.
//!
//! This shell owns filesystem and process work; command and shard planning use values.
//! Each worker runs one case at a time. The slowest case bounds wall time.

mod command;
mod shards;

use std::env;
use std::fs;
use std::io;
use std::num::NonZeroUsize;
use std::path::PathBuf;

#[derive(Debug)]
struct Options {
    subject: PathBuf,
    cases: PathBuf,
    shards: NonZeroUsize,
}

fn parse_options(args: impl IntoIterator<Item = String>) -> io::Result<Options> {
    let mut args = args.into_iter();
    let mut subject = None;
    let mut cases = None;
    let mut shards = NonZeroUsize::new(1).expect("one is non-zero");
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| io::Error::other(format!("missing value for {flag}")))?;
        match flag.as_str() {
            "--workspace" => subject = Some(PathBuf::from(value)),
            "--cases" => cases = Some(PathBuf::from(value)),
            "--shards" => {
                shards = value
                    .parse()
                    .map_err(|_| io::Error::other("--shards needs a positive integer"))?;
            }
            _ => return Err(io::Error::other(format!("unknown option {flag}"))),
        }
    }
    Ok(Options {
        subject: subject.ok_or_else(|| io::Error::other("missing --workspace"))?,
        cases: cases.ok_or_else(|| io::Error::other("missing --cases"))?,
        shards,
    })
}

fn execute(options: Options) -> io::Result<()> {
    let subject = fs::canonicalize(options.subject)?;
    let mut cases = fs::read_dir(options.cases)?
        .map(|entry| entry.and_then(|entry| fs::canonicalize(entry.path())))
        .collect::<io::Result<Vec<_>>>()?;
    cases.sort();
    let root = env::current_dir()?.join(".mutants");
    let shards = shards::plan(&root, options.shards.get(), &cases);
    for result in shards::run(&subject, &shards)? {
        let outcome = if result.caught { "caught" } else { "survived" };
        println!("{}: {outcome}", result.case.display());
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args == ["--help"] {
        println!("rust-mutants --workspace PATH --cases DIRECTORY [--shards N]");
        return Ok(());
    }
    execute(parse_options(args)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_refuses_zero_workers() {
        let args = [
            "--workspace",
            "subject",
            "--cases",
            "cases",
            "--shards",
            "0",
        ];
        assert!(parse_options(args.map(str::to_owned)).is_err());
    }

    #[test]
    fn command_line_defaults_to_one_worker() {
        let args = ["--workspace", "subject", "--cases", "cases"];
        let options = parse_options(args.map(str::to_owned)).expect("valid arguments");
        assert_eq!(options.shards.get(), 1);
        assert_eq!(options.subject, PathBuf::from("subject"));
    }
}
