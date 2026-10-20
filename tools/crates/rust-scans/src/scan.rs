//! The four scans over one parsed file: pure, so each is tested on made-up source with no Cargo and no files.
//!
//! H1 and H4 read paths: every `use` leaf, `extern crate`, and every path in code, types, patterns, attributes and
//! macro names, after [`imports`] resolves its first segment through the file's imports. A path of one segment that
//! no import binds is a local name, such as a variable called `fs` or `tokio`, and is never flagged; a longer path
//! is taken at its word, so a local `mod tokio` called as `tokio::f()` is flagged. Macro arguments are tokens, not
//! syntax, and are not read; nor are re-exports in other files. Raw identifiers are read by their names. H2 reads named fields and the typed
//! parameters of functions and closures, never `let` bindings. H3 reads the tokens of each `#[error(...)]`.

mod imports;

use std::fmt;
use std::num::NonZeroU32;

use camino::Utf8PathBuf;
use proc_macro2::{LineColumn, Spacing, Span, TokenStream, TokenTree};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, FnArg, Ident, Pat, Type};

use self::imports::{Aliases, Import, Segments};
use crate::options::UnitSuffix;
use crate::source::{SourceFile, is_test};

/// One of the scans, printed by its short name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Scan {
    /// H1: file, process, environment and clock access in a core.
    Purity,
    /// H2: a unit-named primitive field or parameter.
    Units,
    /// H3: a code in an error's message.
    Codes,
    /// H4: an async runtime, network or async I/O path, or async syntax, in a core.
    Async,
}

impl Scan {
    /// Every scan, in the order their findings and counts are printed.
    pub(crate) const ALL: [Self; 4] = [Self::Purity, Self::Units, Self::Codes, Self::Async];

    /// Whether a finding in test code is still reported, and marked so.
    fn marks_test_code(self) -> bool {
        match self {
            Self::Purity | Self::Async => true,
            Self::Units | Self::Codes => false,
        }
    }
}

impl fmt::Display for Scan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Purity => "H1",
            Self::Units => "H2",
            Self::Codes => "H3",
            Self::Async => "H4",
        })
    }
}

/// A one-based line number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Line(NonZeroU32);

impl Line {
    /// The line of a parsed token's position; none for a position with no line, or one past `u32`.
    pub(crate) fn from_span(at: LineColumn) -> Option<Self> {
        u32::try_from(at.line)
            .ok()
            .and_then(NonZeroU32::new)
            .map(Self)
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One thing a scan found, printed as `<scan> <file>:<line>: <what>`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Finding {
    scan: Scan,
    file: Utf8PathBuf,
    line: Line,
    what: String,
    test_code: bool,
}

impl Finding {
    pub(crate) fn scan(&self) -> Scan {
        self.scan
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}:{}: {}",
            self.scan, self.file, self.line, self.what
        )?;
        if self.test_code && self.scan.marks_test_code() {
            f.write_str(" (test code)")?;
        }
        Ok(())
    }
}

/// The unit suffixes H2 reads: the built-in ones and a project's own.
#[derive(Debug)]
pub(crate) struct Rules {
    suffixes: Vec<Box<str>>,
}

impl Rules {
    const BUILT_IN: [&str; 5] = ["metres", "seconds", "degrees", "radians", "hz"];

    /// The built-in suffixes extended, never replaced, by `units`.
    pub(crate) fn new(units: &[UnitSuffix]) -> Self {
        let mut suffixes: Vec<Box<str>> =
            Self::BUILT_IN.iter().map(|&suffix| suffix.into()).collect();
        for unit in units {
            if !suffixes.iter().any(|known| **known == *unit.as_str()) {
                suffixes.push(unit.as_str().into());
            }
        }
        Self { suffixes }
    }

    /// Whether `name` is a lowercase name ending in `_` and a unit suffix, with something before that `_`.
    fn unit_named(&self, name: &str) -> bool {
        // An identifier cannot start with a digit, so these characters alone make a lowercase name.
        let lowercase = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        lowercase
            && self.suffixes.iter().any(|suffix| {
                name.strip_suffix(&**suffix)
                    .and_then(|stem| stem.strip_suffix('_'))
                    .is_some_and(|stem| !stem.is_empty())
            })
    }
}

const PRIMITIVES: [&str; 14] = [
    "f32", "f64", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128",
    "isize",
];

/// H1's paths, each with the name its findings print.
const PURITY: [(&[&str], &str); 5] = [
    (&["std", "fs"], "std::fs"),
    (&["std", "process"], "std::process"),
    (&["std", "env"], "std::env"),
    (&["std", "time", "Instant"], "Instant"),
    (&["std", "time", "SystemTime"], "SystemTime"),
];

/// H4's paths: async runtimes, network clients and async I/O.
const ASYNC: [&[&str]; 12] = [
    &["tokio"],
    &["async_std"],
    &["smol"],
    &["futures", "io"],
    &["std", "net"],
    &["reqwest"],
    &["hyper"],
    &["h3"],
    &["quinn"],
    &["wtransport"],
    &["sqlx"],
    &["libsql"],
];

/// Every finding in one parsed file, in line order.
pub(crate) fn scan_file(source: &syn::File, file: &SourceFile, rules: &Rules) -> Vec<Finding> {
    let mut scanner = Scanner {
        file,
        rules,
        aliases: Aliases::of(source),
        in_test: 0,
        findings: Vec::new(),
    };
    scanner.visit_file(source);
    scanner.findings.sort_by_key(|finding| finding.line);
    scanner.findings
}

struct Scanner<'a> {
    file: &'a SourceFile,
    rules: &'a Rules,
    aliases: Aliases,
    in_test: usize,
    findings: Vec<Finding>,
}

impl Scanner<'_> {
    fn test_code(&self) -> bool {
        self.file.test_code() || self.in_test > 0
    }

    fn found(&mut self, scan: Scan, at: Span, what: String) {
        let line = Line::from_span(at.start()).expect("a token parsed from a file has a line");
        let test_code = self.test_code();
        self.findings.push(Finding {
            scan,
            file: self.file.shown().to_owned(),
            line,
            what,
            test_code,
        });
    }

    /// Visits what `attributes` sit on, as test code when they put it under `cfg(test)`.
    fn under(&mut self, attributes: &[Attribute], visit: impl FnOnce(&mut Self)) {
        let test = usize::from(is_test(attributes));
        self.in_test += test;
        visit(self);
        self.in_test -= test;
    }

    /// H1 and H4 on a path, as written: a path of one segment counts only when an import resolves it or it is
    /// itself imported.
    fn path(&mut self, written: &[String], span: Span, imported: bool) {
        if !self.file.core() {
            return;
        }
        let resolved = self.aliases.resolve(written);
        let counted = |path: &&Segments| imported || path.len() > 1;
        if let Some(name) = resolved
            .iter()
            .filter(counted)
            .find_map(|path| purity(path))
        {
            let what = format!("{name} in {}", self.file.crate_name());
            self.found(Scan::Purity, span, what);
        }
        if let Some(family) = resolved
            .iter()
            .filter(counted)
            .find_map(|path| asynchronous(path))
        {
            let what = format!("{} in {}", family.join("::"), self.file.crate_name());
            self.found(Scan::Async, span, what);
        }
    }

    /// H1 and H4 on a glob import: each flagged path it is inside, or brings into scope by name.
    fn glob(&mut self, written: &[String], span: Span) {
        if !self.file.core() {
            return;
        }
        for resolved in self.aliases.resolve(written) {
            let reaches =
                |flagged: &[&str]| starts_with(&resolved, flagged) || holds(&resolved, flagged);
            for (flagged, name) in PURITY {
                if reaches(flagged) {
                    let what = format!("{name} in {}", self.file.crate_name());
                    self.found(Scan::Purity, span, what);
                }
            }
            for flagged in ASYNC {
                if reaches(flagged) {
                    let what = format!("{} in {}", flagged.join("::"), self.file.crate_name());
                    self.found(Scan::Async, span, what);
                }
            }
        }
    }

    fn asynchronous_syntax(&mut self, span: Span, what: &str) {
        if self.file.core() {
            let what = format!("{what} in {}", self.file.crate_name());
            self.found(Scan::Async, span, what);
        }
    }

    /// H2 on a field or parameter named `ident` of type `ty`.
    fn unit(&mut self, ident: &Ident, ty: &Type) {
        if self.test_code() || self.file.kernel() {
            return;
        }
        let name = ident.unraw().to_string();
        if let Some(primitive) = primitive(ty)
            && self.rules.unit_named(&name)
        {
            self.found(Scan::Units, ident.span(), format!("{name}: {primitive}"));
        }
    }

    /// H2 on a typed parameter that is a plain name, `mut` or not.
    fn parameter(&mut self, pat: &Pat, ty: &Type) {
        if let Pat::Ident(binding) = pat
            && binding.subpat.is_none()
        {
            self.unit(&binding.ident, ty);
        }
    }
}

fn starts_with(path: &[String], prefix: &[&str]) -> bool {
    path.len() >= prefix.len()
        && path
            .iter()
            .zip(prefix)
            .all(|(segment, wanted)| segment == wanted)
}

/// Whether `flagged` is one segment below `path`, so a glob of `path` brings it in by name.
fn holds(path: &[String], flagged: &[&str]) -> bool {
    flagged.len() == path.len() + 1 && starts_with(path, &flagged[..path.len()])
}

fn purity(path: &[String]) -> Option<&'static str> {
    PURITY
        .iter()
        .find(|(flagged, _)| starts_with(path, flagged))
        .map(|&(_, name)| name)
}

fn asynchronous(path: &[String]) -> Option<&'static [&'static str]> {
    ASYNC
        .iter()
        .copied()
        .find(|flagged| starts_with(path, flagged))
}

/// The primitive a type names, alone or borrowed once.
fn primitive(ty: &Type) -> Option<&'static str> {
    let named = match ty {
        Type::Reference(reference) => &*reference.elem,
        other => other,
    };
    let Type::Path(typed) = named else {
        return None;
    };
    let ident = typed
        .qself
        .is_none()
        .then(|| typed.path.get_ident())
        .flatten()?;
    PRIMITIVES.iter().copied().find(|known| ident == known)
}

/// Whether an `#[error(...)]`'s tokens put a code in the message: `{code}` or `{code:...}` in a string, or a named
/// argument `code = ...` at the start of an argument. A field read in a positional argument, such as `.code` or
/// `code == 1`, is not recognised.
fn has_code(tokens: TokenStream) -> bool {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    tokens.iter().enumerate().any(|(index, token)| match token {
        TokenTree::Literal(literal) => match syn::Lit::new(literal.clone()) {
            syn::Lit::Str(text) => {
                let text = text.value();
                text.contains("{code}") || text.contains("{code:")
            }
            _ => false,
        },
        TokenTree::Ident(ident) if ident.unraw() == "code" => {
            // A valid #[error] list opens with its message or `transparent`, so an argument follows a comma.
            let starts_argument = matches!(
                index.checked_sub(1).and_then(|before| tokens.get(before)),
                Some(TokenTree::Punct(punct)) if punct.as_char() == ','
            );
            // `code = ...` names an argument; the `=` of `code == ...`, a comparison, is joined to the next one.
            let assigned = matches!(
                tokens.get(index + 1),
                Some(TokenTree::Punct(punct)) if punct.as_char() == '=' && punct.spacing() == Spacing::Alone
            );
            starts_argument && assigned
        }
        TokenTree::Group(_) | TokenTree::Ident(_) | TokenTree::Punct(_) => false,
    })
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let attributes: &[Attribute] = match item {
            syn::Item::Const(item) => &item.attrs,
            syn::Item::Enum(item) => &item.attrs,
            syn::Item::ExternCrate(item) => &item.attrs,
            syn::Item::Fn(item) => &item.attrs,
            syn::Item::ForeignMod(item) => &item.attrs,
            syn::Item::Impl(item) => &item.attrs,
            syn::Item::Macro(item) => &item.attrs,
            syn::Item::Mod(item) => &item.attrs,
            syn::Item::Static(item) => &item.attrs,
            syn::Item::Struct(item) => &item.attrs,
            syn::Item::Trait(item) => &item.attrs,
            syn::Item::TraitAlias(item) => &item.attrs,
            syn::Item::Type(item) => &item.attrs,
            syn::Item::Union(item) => &item.attrs,
            syn::Item::Use(item) => &item.attrs,
            _ => &[],
        };
        self.under(attributes, |scanner| visit::visit_item(scanner, item));
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attributes: &[Attribute] = match item {
            syn::ImplItem::Const(item) => &item.attrs,
            syn::ImplItem::Fn(item) => &item.attrs,
            syn::ImplItem::Type(item) => &item.attrs,
            syn::ImplItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.under(attributes, |scanner| visit::visit_impl_item(scanner, item));
    }

    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let attributes: &[Attribute] = match item {
            syn::TraitItem::Const(item) => &item.attrs,
            syn::TraitItem::Fn(item) => &item.attrs,
            syn::TraitItem::Type(item) => &item.attrs,
            syn::TraitItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.under(attributes, |scanner| visit::visit_trait_item(scanner, item));
    }

    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        let attributes: &[Attribute] = match item {
            syn::ForeignItem::Fn(item) => &item.attrs,
            syn::ForeignItem::Static(item) => &item.attrs,
            syn::ForeignItem::Type(item) => &item.attrs,
            syn::ForeignItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.under(attributes, |scanner| {
            visit::visit_foreign_item(scanner, item);
        });
    }

    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        self.under(&variant.attrs, |scanner| {
            visit::visit_variant(scanner, variant);
        });
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        self.under(&field.attrs, |scanner| {
            if let Some(ident) = &field.ident {
                scanner.unit(ident, &field.ty);
            }
            visit::visit_field(scanner, field);
        });
    }

    fn visit_fn_arg(&mut self, argument: &'ast FnArg) {
        if let FnArg::Typed(typed) = argument {
            self.parameter(&typed.pat, &typed.ty);
        }
        visit::visit_fn_arg(self, argument);
    }

    fn visit_named_arg(&mut self, argument: &'ast syn::NamedArg) {
        if let Some((name, _)) = &argument.name {
            self.unit(name, &argument.ty);
        }
        visit::visit_named_arg(self, argument);
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        for input in &closure.inputs {
            if let Pat::Type(typed) = input {
                self.parameter(&typed.pat, &typed.ty);
            }
        }
        if let Some(asyncness) = &closure.asyncness {
            self.asynchronous_syntax(asyncness.span, "async closure");
        }
        visit::visit_expr_closure(self, closure);
    }

    fn visit_signature(&mut self, signature: &'ast syn::Signature) {
        if let Some(asyncness) = &signature.asyncness {
            self.asynchronous_syntax(asyncness.span, "async fn");
        }
        visit::visit_signature(self, signature);
    }

    fn visit_expr_async(&mut self, block: &'ast syn::ExprAsync) {
        self.asynchronous_syntax(block.async_token.span, "async block");
        visit::visit_expr_async(self, block);
    }

    fn visit_expr_await(&mut self, awaited: &'ast syn::ExprAwait) {
        self.asynchronous_syntax(awaited.await_token.span, ".await");
        visit::visit_expr_await(self, awaited);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let start = self.findings.len();
        for leaf in imports::leaves(&item.tree) {
            match leaf.import {
                Import::Glob => self.glob(&leaf.path, leaf.span),
                Import::Name(_) | Import::Hidden => self.path(&leaf.path, leaf.span, true),
            }
        }
        // One import of several names from one flagged module, such as `use std::fs::{self, File}`, is one finding.
        let found = self.findings.split_off(start);
        for finding in found {
            let repeated = self.findings[start..]
                .iter()
                .any(|kept| kept.scan == finding.scan && kept.what == finding.what);
            if !repeated {
                self.findings.push(finding);
            }
        }
        visit::visit_item_use(self, item);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        self.path(&[item.ident.unraw().to_string()], item.ident.span(), true);
        visit::visit_item_extern_crate(self, item);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if self.file.core() {
            let written: Segments = path
                .segments
                .iter()
                .map(|segment| segment.ident.unraw().to_string())
                .collect();
            self.path(&written, path.span(), false);
        }
        visit::visit_path(self, path);
    }

    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        if attribute.path().is_ident("error")
            && let syn::Meta::List(list) = &attribute.meta
            && has_code(list.tokens.clone())
        {
            self.found(
                Scan::Codes,
                attribute.pound_token.span,
                "a code in #[error(...)]".to_owned(),
            );
        }
        visit::visit_attribute(self, attribute);
    }
}

/// Each scan's promises on made-up source, each expected line written by hand from the source above it.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::CrateName;

    /// How a made-up file sits in its crate.
    #[derive(Clone, Copy)]
    struct Place {
        core: bool,
        test_code: bool,
        kernel: bool,
    }

    const CORE: Place = Place {
        core: true,
        test_code: false,
        kernel: false,
    };
    const SHELL: Place = Place {
        core: false,
        test_code: false,
        kernel: false,
    };

    fn findings_with(text: &str, place: Place, units: &[&str]) -> Vec<String> {
        let source = syn::parse_file(text).expect("made-up source parses");
        let name = CrateName::new("toy").expect("a crate name");
        let file = SourceFile::new(
            "src/lib.rs".into(),
            name,
            place.test_code,
            place.kernel,
            place.core,
        );
        let units: Vec<UnitSuffix> = units
            .iter()
            .map(|unit| UnitSuffix::new(unit).expect("a unit"))
            .collect();
        scan_file(&source, &file, &Rules::new(&units))
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn findings(text: &str, place: Place) -> Vec<String> {
        findings_with(text, place, &[])
    }

    fn only(text: &str, place: Place, scan: Scan) -> Vec<String> {
        let prefix = format!("{scan} ");
        findings(text, place)
            .into_iter()
            .filter(|finding| finding.starts_with(&prefix))
            .collect()
    }

    // H1

    #[test]
    fn h1_reports_file_process_environment_and_clock_paths_in_a_core() {
        let text = "\
use std::fs;
fn run() {
    let _ = std::env::var(\"HOME\");
    std::process::exit(1);
    let _ = std::time::Instant::now();
    let _ = std::time::SystemTime::now();
}
";
        assert_eq!(
            only(text, CORE, Scan::Purity),
            [
                "H1 src/lib.rs:1: std::fs in toy",
                "H1 src/lib.rs:3: std::env in toy",
                "H1 src/lib.rs:4: std::process in toy",
                "H1 src/lib.rs:5: Instant in toy",
                "H1 src/lib.rs:6: SystemTime in toy",
            ]
        );
    }

    #[test]
    fn h1_reads_only_the_crates_named_as_cores() {
        assert_eq!(
            findings(
                "use std::fs; fn f() -> std::time::Instant { todo!() }",
                SHELL
            ),
            [""; 0]
        );
    }

    #[test]
    fn h1_follows_a_renamed_or_grouped_import_to_its_use() {
        let text = "\
use std::fs as files;
use std::{env as vars, time::Instant};
fn run() {
    files::read(\"a\");
    vars::var(\"b\");
    let _: Instant;
}
";
        assert_eq!(
            only(text, CORE, Scan::Purity),
            [
                "H1 src/lib.rs:1: std::fs in toy",
                "H1 src/lib.rs:2: std::env in toy",
                "H1 src/lib.rs:2: Instant in toy",
                "H1 src/lib.rs:4: std::fs in toy",
                "H1 src/lib.rs:5: std::env in toy",
                "H1 src/lib.rs:6: Instant in toy",
            ]
        );
    }

    #[test]
    fn a_raw_identifier_is_read_as_its_name() {
        let text = "\
fn run() {
    std::r#fs::read(\"a\");
    r#tokio::spawn(work());
}
";
        assert_eq!(
            findings(text, CORE),
            [
                "H1 src/lib.rs:2: std::fs in toy",
                "H4 src/lib.rs:3: tokio in toy"
            ]
        );
    }

    #[test]
    fn h1_leaves_out_names_spelt_like_its_paths_that_are_not_them() {
        let text = "\
struct Instant;
mod fs { pub fn read() {} }
fn run(env: u8) -> Instant {
    let fs = 1;
    crate::fs::read();
    let _ = \"std::fs\";
    // std::process::exit(1)
    let _ = env + fs;
    Instant
}
";
        assert_eq!(findings(text, CORE), [""; 0]);
    }

    #[test]
    fn h1_reports_one_import_of_several_names_from_one_module_once() {
        let text = "use std::{fs::{self, File, OpenOptions}, env};\n";
        assert_eq!(
            only(text, CORE, Scan::Purity),
            [
                "H1 src/lib.rs:1: std::fs in toy",
                "H1 src/lib.rs:1: std::env in toy",
            ]
        );
    }

    #[test]
    fn h1_reports_a_glob_by_the_path_it_imports() {
        let text = "use std::fs::*;\nuse std::time::*;\n";
        assert_eq!(
            only(text, CORE, Scan::Purity),
            [
                "H1 src/lib.rs:1: std::fs in toy",
                "H1 src/lib.rs:2: Instant in toy",
                "H1 src/lib.rs:2: SystemTime in toy",
            ]
        );
    }

    #[test]
    fn h1_marks_test_code_inline_and_by_file() {
        let text = "\
#[cfg(test)]
mod tests {
    fn fixture() { let _ = std::env::var(\"HOME\"); }
}
#[cfg(test)]
fn helper() { std::fs::read(\"a\"); }
";
        assert_eq!(
            only(text, CORE, Scan::Purity),
            [
                "H1 src/lib.rs:3: std::env in toy (test code)",
                "H1 src/lib.rs:6: std::fs in toy (test code)",
            ]
        );
        let in_test_file = Place {
            test_code: true,
            ..CORE
        };
        assert_eq!(
            only("use std::fs;", in_test_file, Scan::Purity),
            ["H1 src/lib.rs:1: std::fs in toy (test code)"]
        );
    }

    // H2

    #[test]
    fn h2_reports_unit_named_primitive_fields_and_parameters() {
        let text = "\
pub struct Reach { pub margin_metres: f64, count: u8 }
enum Pace { Steady { stride_metres: f32 } }
fn wait(timeout_seconds: f64, mut turn_degrees: f32, rate_hz: &u32, phase_radians: &'static mut f64) {}
fn each() { let _ = |gap_metres: f64| gap_metres; }
pub type Callback = fn(timeout_seconds: f64);
pub struct Timing { pub r#delay_seconds: f64 }
";
        assert_eq!(
            only(text, SHELL, Scan::Units),
            [
                "H2 src/lib.rs:1: margin_metres: f64",
                "H2 src/lib.rs:2: stride_metres: f32",
                "H2 src/lib.rs:3: timeout_seconds: f64",
                "H2 src/lib.rs:3: turn_degrees: f32",
                "H2 src/lib.rs:3: rate_hz: u32",
                "H2 src/lib.rs:3: phase_radians: f64",
                "H2 src/lib.rs:4: gap_metres: f64",
                "H2 src/lib.rs:5: timeout_seconds: f64",
                "H2 src/lib.rs:6: delay_seconds: f64",
            ]
        );
    }

    #[test]
    fn h2_leaves_out_lets_unit_types_other_names_and_paths() {
        let text = "\
struct Gap { width_metres: Length, metres: f64, metres_total: f64, _metres: f64, Span_metres: f64 }
fn f(spans: (f64, f64), every_seconds: Vec<f64>, at_seconds: std::primitive::f64, by_hz: &&u8) {
    let gap_metres: f64 = f64::MAX;
}
type Unnamed = fn(f64, _: f64, wait_seconds: Duration);
";
        assert_eq!(only(text, SHELL, Scan::Units), [""; 0]);
    }

    #[test]
    fn h2_leaves_out_test_code_and_kernels() {
        let text = "\
#[cfg(test)]
mod tests { fn fixture(span_seconds: f64) {} }
pub fn step(dt_seconds: f64) {}
";
        assert_eq!(
            only(text, SHELL, Scan::Units),
            ["H2 src/lib.rs:3: dt_seconds: f64"]
        );
        assert_eq!(
            only(
                text,
                Place {
                    kernel: true,
                    ..SHELL
                },
                Scan::Units
            ),
            [""; 0]
        );
        assert_eq!(
            only(
                text,
                Place {
                    test_code: true,
                    ..SHELL
                },
                Scan::Units
            ),
            [""; 0]
        );
    }

    #[test]
    fn h2_project_units_extend_the_built_in_ones() {
        let text = "struct Push { force_newtons: f64, hold_seconds: f64, area_m2: f64 }";
        let found = findings_with(text, SHELL, &["newtons", "m2"]);
        assert_eq!(
            found,
            [
                "H2 src/lib.rs:1: force_newtons: f64",
                "H2 src/lib.rs:1: hold_seconds: f64",
                "H2 src/lib.rs:1: area_m2: f64",
            ]
        );
    }

    // H3

    #[test]
    fn h3_reports_a_code_in_an_error_message_at_the_attributes_first_line() {
        let text = "\
#[derive(Debug, thiserror::Error)]
enum ReadError {
    #[error(
        \"{code}: could not read {path}\"
    )]
    Missing { path: String, code: &'static str },
    #[error(\"[{code:>4}] could not write\")]
    Full { code: u16 },
    #[error(\"{}: no room\", code = self.kind)]
    Room { kind: u8 },
    #[error(\"could not write {path}\", path = self.path)]
    Write { path: String },
    #[error(\"{codes} were refused\")]
    Refused { codes: u8 },
    #[error(\"{} was refused\", .code.kind)]
    Positional { code: Kind },
    #[error(\"unexpected reply, matches expected: {}\", .code == .expected)]
    Compared { code: u16, expected: u16 },
    #[error(\"unexpected reply, matches: {}\", code == 1)]
    Bound { code: u16 },
}
";
        assert_eq!(
            only(text, SHELL, Scan::Codes),
            [
                "H3 src/lib.rs:3: a code in #[error(...)]",
                "H3 src/lib.rs:7: a code in #[error(...)]",
                "H3 src/lib.rs:9: a code in #[error(...)]",
            ]
        );
    }

    #[test]
    fn h3_reads_test_code_and_kernels_unmarked() {
        let text = "#[cfg(test)]\nmod tests {\n    #[error(\"{code}\")]\n    struct Coded;\n}\n";
        let place = Place {
            test_code: true,
            kernel: true,
            ..SHELL
        };
        assert_eq!(
            only(text, place, Scan::Codes),
            ["H3 src/lib.rs:3: a code in #[error(...)]"]
        );
    }

    // H4

    #[test]
    fn h4_reports_runtime_network_and_async_io_paths_in_a_core() {
        let text = "\
use tokio::sync::mpsc;
use std::net::TcpStream;
fn run() {
    let _ = futures::io::empty();
    let _ = reqwest::get(\"x\");
}
#[tokio::main]
fn start() {}
extern crate tokio;
";
        assert_eq!(
            only(text, CORE, Scan::Async),
            [
                "H4 src/lib.rs:1: tokio in toy",
                "H4 src/lib.rs:2: std::net in toy",
                "H4 src/lib.rs:4: futures::io in toy",
                "H4 src/lib.rs:5: reqwest in toy",
                "H4 src/lib.rs:7: tokio in toy",
                "H4 src/lib.rs:9: tokio in toy",
            ]
        );
    }

    #[test]
    fn h4_reports_each_runtime_network_and_storage_family() {
        let cases = [
            ("async_std::task::spawn", Some("async_std")),
            ("smol::block_on", Some("smol")),
            ("hyper::Request", Some("hyper")),
            ("h3::client::new", Some("h3")),
            ("quinn::Endpoint", Some("quinn")),
            ("wtransport::Endpoint", Some("wtransport")),
            ("sqlx::query", Some("sqlx")),
            ("libsql::Builder", Some("libsql")),
            ("smolstr::SmolStr", None),
            ("h3o::CellIndex", None),
            ("hyperloglog::HyperLogLog", None),
            ("async_stream::stream", None),
            ("std::num::NonZeroU8", None),
            ("futures::stream::empty", None),
        ];
        for (path, family) in cases {
            let expected: Vec<String> = family
                .map(|family| format!("H4 src/lib.rs:1: {family} in toy"))
                .into_iter()
                .collect();
            assert_eq!(
                findings(&format!("fn run() {{ let _ = {path}; }}"), CORE),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn h4_follows_a_renamed_import() {
        let text = "use tokio as runtime;\nfn run() { runtime::spawn(work()); }\n";
        assert_eq!(
            only(text, CORE, Scan::Async),
            [
                "H4 src/lib.rs:1: tokio in toy",
                "H4 src/lib.rs:2: tokio in toy"
            ]
        );
    }

    #[test]
    fn h4_reports_async_syntax() {
        let text = "\
async fn fetch() {}
fn run() {
    let later = async { fetch().await };
    let call = async || 1;
}
";
        assert_eq!(
            only(text, CORE, Scan::Async),
            [
                "H4 src/lib.rs:1: async fn in toy",
                "H4 src/lib.rs:3: async block in toy",
                "H4 src/lib.rs:3: .await in toy",
                "H4 src/lib.rs:4: async closure in toy",
            ]
        );
    }

    #[test]
    fn h4_leaves_out_names_spelt_like_its_paths_and_other_crates() {
        let text = "\
mod net { pub fn send() {} }
fn run(tokio: u8, smol: u8) -> u8 {
    net::send();
    let _ = futures::stream::empty::<u8>();
    let _ = crate::hyper::Body;
    tokio + smol
}
";
        assert_eq!(only(text, CORE, Scan::Async), [""; 0]);
        assert_eq!(
            findings("async fn f() { g().await }\nuse tokio::spawn;", SHELL),
            [""; 0]
        );
    }

    #[test]
    fn h4_marks_test_code() {
        let text = "#[cfg(test)]\nmod tests {\n    async fn fixture() {}\n}\n";
        assert_eq!(
            only(text, CORE, Scan::Async),
            ["H4 src/lib.rs:3: async fn in toy (test code)"]
        );
    }

    #[test]
    fn a_line_is_one_based_and_needs_a_position() {
        assert_eq!(
            Line::from_span(LineColumn { line: 1, column: 0 }).map(|line| line.to_string()),
            Some("1".to_owned())
        );
        assert_eq!(Line::from_span(LineColumn { line: 0, column: 0 }), None);
    }

    #[test]
    fn every_kind_of_item_under_cfg_test_is_test_code() {
        let text = "\
#[cfg(test)] const A: Option<std::fs::File> = None;
#[cfg(test)] enum B { V(std::fs::File) }
#[cfg(test)] extern crate tokio;
#[cfg(test)] fn c(_: std::fs::File) {}
#[cfg(test)] extern \"C\" { fn d(_: std::fs::File); }
#[cfg(test)] impl std::fs::File {}
#[cfg(test)] std::fs::e!();
#[cfg(test)] mod f { use std::fs; }
#[cfg(test)] static G: Option<std::fs::File> = None;
#[cfg(test)] struct H(std::fs::File);
#[cfg(test)] trait I: std::fs::Bound {}
#[cfg(test)] trait J = std::fs::Bound;
#[cfg(test)] type K = std::fs::File;
#[cfg(test)] union L { l: std::fs::File }
#[cfm(test)] use std::fs::read;
#[cfg(test)] use std::fs::write;
impl S {
    #[cfg(test)] const M: Option<std::fs::File> = None;
    #[cfg(test)] fn n(_: std::fs::File) {}
    #[cfg(test)] type O = std::fs::File;
    #[cfg(test)] std::fs::p!();
}
trait Q {
    #[cfg(test)] const R: std::fs::File;
    #[cfg(test)] fn s(_: std::fs::File);
    #[cfg(test)] type T: std::fs::Bound;
    #[cfg(test)] std::fs::u!();
}
extern \"C\" {
    #[cfg(test)] fn v(_: std::fs::File);
    #[cfg(test)] static W: std::fs::File;
    #[cfg(test)] #[std::fs::marked] type X;
    #[cfg(test)] std::fs::y!();
}
enum Z { #[cfg(test)] Variant(std::fs::File) }
struct Fields { #[cfg(test)] field: std::fs::File }
";
        let found = findings(text, CORE);
        let unmarked: Vec<&String> = found
            .iter()
            .filter(|finding| !finding.ends_with(" (test code)"))
            .collect();
        assert_eq!(unmarked, ["H1 src/lib.rs:15: std::fs in toy"]);
        assert_eq!(found.len(), 30);
    }
}
