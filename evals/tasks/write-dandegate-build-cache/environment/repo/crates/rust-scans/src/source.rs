//! What a source file is to the scans: which crate it belongs to, whether it is test code, a kernel's or a core's.
//!
//! Test code is told apart from what is left out of a scan: H1 and H4 still read test code and mark it, H2 leaves it
//! out, and H3 reads it unmarked. A whole file is test code when it sits under a `tests` folder below `src/`, is named
//! `tests.rs` or `*_tests.rs`, or is a module some file of its crate declares under `#[cfg(test)]`, with everything
//! below that module's folder. Inside any file, an item, associated item, field or variant under `#[cfg(test)]`, or
//! under a `cfg(all(...))` that requires `test`, is test code with everything in it.

use camino::{Utf8Path, Utf8PathBuf};
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Meta, Token};

use crate::options::CrateName;

/// One source file of a member crate, with the facts the scans read about it.
#[derive(Debug)]
pub(crate) struct SourceFile {
    shown: Utf8PathBuf,
    crate_name: CrateName,
    test_code: bool,
    kernel: bool,
    core: bool,
}

impl SourceFile {
    /// A file as its findings name it (`shown`), in `crate_name`, and whether the whole file is test code, inside a
    /// kernel module, and in a core crate: three independent facts.
    pub(crate) fn new(
        shown: Utf8PathBuf,
        crate_name: CrateName,
        test_code: bool,
        kernel: bool,
        core: bool,
    ) -> Self {
        Self {
            shown,
            crate_name,
            test_code,
            kernel,
            core,
        }
    }

    pub(crate) fn shown(&self) -> &Utf8Path {
        &self.shown
    }

    pub(crate) fn crate_name(&self) -> &CrateName {
        &self.crate_name
    }

    pub(crate) fn test_code(&self) -> bool {
        self.test_code
    }

    pub(crate) fn kernel(&self) -> bool {
        self.kernel
    }

    pub(crate) fn core(&self) -> bool {
        self.core
    }
}

/// Whether a file, by its path under `src/`, is test code by its name or folder alone.
pub(crate) fn test_file(under_src: &Utf8Path) -> bool {
    let name = under_src.file_name().unwrap_or_default();
    name == "tests.rs"
        || name.ends_with("_tests.rs")
        || under_src
            .parent()
            .is_some_and(|folder| folder.components().any(|part| part.as_str() == "tests"))
}

/// Whether `file` is the module at `module`, a path without `.rs`, as its file or anywhere below its folder; a sibling
/// whose name only starts with the module's is not.
pub(crate) fn within(file: &Utf8Path, module: &Utf8Path) -> bool {
    file.with_extension("") == module || file.starts_with(module)
}

/// The folder a file's `mod name;` declarations are found in: beside a crate root or a `mod.rs`, otherwise the folder
/// named after the file.
pub(crate) fn module_folder(file: &Utf8Path, roots: &[Utf8PathBuf]) -> Utf8PathBuf {
    let beside = roots.iter().any(|root| root == file) || file.file_name() == Some("mod.rs");
    match file.parent() {
        Some(parent) if beside => parent.to_owned(),
        _ => file.with_extension(""),
    }
}

/// Every out-of-line module `source` declares under `#[cfg(test)]`, or inside an inline test module, as a path
/// without `.rs` below `folder`, the file's [`module_folder`].
pub(crate) fn declared_test_modules(source: &syn::File, folder: Utf8PathBuf) -> Vec<Utf8PathBuf> {
    let mut declared = Declared {
        folder,
        inline: Vec::new(),
        in_test: 0,
        found: Vec::new(),
    };
    declared.visit_file(source);
    declared.found
}

struct Declared {
    folder: Utf8PathBuf,
    inline: Vec<String>,
    in_test: usize,
    found: Vec<Utf8PathBuf>,
}

impl<'ast> Visit<'ast> for Declared {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        let test = is_test(&module.attrs);
        let name = module.ident.unraw().to_string();
        if module.content.is_none() {
            if test || self.in_test > 0 {
                let mut path = self.folder.clone();
                path.extend(&self.inline);
                path.push(name);
                self.found.push(path);
            }
            return;
        }
        self.inline.push(name);
        self.in_test += usize::from(test);
        visit::visit_item_mod(self, module);
        self.in_test -= usize::from(test);
        self.inline.pop();
    }
}

/// Whether the attributes put what they are on under `cfg(test)`, alone or required by `cfg(all(...))`.
pub(crate) fn is_test(attributes: &[Attribute]) -> bool {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"))
        .filter_map(|attribute| attribute.parse_args::<Meta>().ok())
        .any(|predicate| requires_test(&predicate))
}

fn requires_test(predicate: &Meta) -> bool {
    match predicate {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .is_ok_and(|all| all.iter().any(requires_test)),
        Meta::List(_) | Meta::NameValue(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> syn::File {
        syn::parse_file(text).expect("made-up source parses")
    }

    #[test]
    fn a_file_under_tests_or_named_for_tests_is_test_code() {
        for path in [
            "tests.rs",
            "solve_tests.rs",
            "tests/fixtures.rs",
            "arms/tests/sqp.rs",
        ] {
            assert!(test_file(Utf8Path::new(path)), "{path}");
        }
        for path in ["lib.rs", "tests_helper.rs", "contests.rs", "arms/sqp.rs"] {
            assert!(!test_file(Utf8Path::new(path)), "{path}");
        }
    }

    #[test]
    fn a_module_covers_its_file_and_folder_but_not_a_similarly_named_sibling() {
        let module = Utf8Path::new("src/arms/sqp");
        assert!(within(Utf8Path::new("src/arms/sqp.rs"), module));
        assert!(within(Utf8Path::new("src/arms/sqp/steps.rs"), module));
        assert!(!within(Utf8Path::new("src/arms/sqp_extra.rs"), module));
        assert!(!within(Utf8Path::new("src/arms.rs"), module));
    }

    #[test]
    fn children_are_beside_a_crate_root_or_mod_rs_and_otherwise_in_the_files_folder() {
        let roots = [
            Utf8PathBuf::from("src/lib.rs"),
            Utf8PathBuf::from("src/bin/tool.rs"),
        ];
        assert_eq!(module_folder(Utf8Path::new("src/lib.rs"), &roots), "src");
        assert_eq!(
            module_folder(Utf8Path::new("src/bin/tool.rs"), &roots),
            "src/bin"
        );
        assert_eq!(
            module_folder(Utf8Path::new("src/arms/mod.rs"), &roots),
            "src/arms"
        );
        assert_eq!(
            module_folder(Utf8Path::new("src/arms.rs"), &roots),
            "src/arms"
        );
    }

    #[test]
    fn modules_declared_under_cfg_test_or_inside_an_inline_test_module_are_test_modules() {
        let source = parsed(
            "#[cfg(test)] mod tests;\n\
             mod solve;\n\
             #[cfg(all(test, feature = \"slow\"))] mod slow;\n\
             #[cfg(any(test, feature = \"x\"))] mod either;\n\
             mod outer { #[cfg(test)] mod fixtures; mod plain; }\n\
             #[cfg(test)] mod support { mod made_up; }\n\
             mod later;\n\
             #[cfg(test)] mod r#type;\n",
        );
        let found = declared_test_modules(&source, Utf8PathBuf::from("src"));
        let expected = [
            "src/tests",
            "src/slow",
            "src/outer/fixtures",
            "src/support/made_up",
            "src/type",
        ];
        assert_eq!(found, expected.map(Utf8PathBuf::from));
    }
}
