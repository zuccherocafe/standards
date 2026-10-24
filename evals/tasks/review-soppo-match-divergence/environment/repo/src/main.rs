//! Load a Soppo package and print typechecking diagnostics.

use std::process::ExitCode;

use soppo::infer::check_source;

fn main() -> ExitCode {
    let Some(package) = std::env::args().nth(1) else {
        eprintln!("usage: soppo <package.soppo>");
        return ExitCode::FAILURE;
    };
    let Ok(source) = std::fs::read_to_string(&package) else {
        eprintln!("{package}: source unavailable");
        return ExitCode::FAILURE;
    };
    match check_source(&source) {
        Ok(report) if report.diagnostics.is_empty() => ExitCode::SUCCESS,
        Ok(report) => {
            eprintln!("{}: {:?}", report.function, report.diagnostics);
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("{package}: {error:?}");
            ExitCode::FAILURE
        }
    }
}
