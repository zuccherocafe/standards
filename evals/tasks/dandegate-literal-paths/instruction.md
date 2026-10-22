# Read dandegate's repository paths literally

`tools/crates/dandegate` is dandegate, a command that reports what a change moved between two revisions of a project. A project's `dandegate.toml` names repository paths in two places: the measuring code, `code` under `[reports.measures]`, whose entries are each a file or a folder, and the Cargo workspace's folder, `workspace` under `[project.rust]`. dandegate asks git which of these paths a change touched, to warn that measuring code changed and to decide whether any Rust in the workspace differs, which decides whether the mutation report runs.

Each of these paths names one repository file or folder by its literal name. A file selects a change to itself and nothing else. A folder selects a change to any file below it and nothing else. This holds whatever characters a valid file or folder name contains, including those git would otherwise read as pattern syntax, such as `*`, `?`, `[` and `]`. dandegate does not keep this promise today. Make it keep it.

Everything else dandegate does stays as it is, and its existing tests must still pass. You need not add tests, though you may.

`AGENTS.md` at the repository root says how to build and test it.
