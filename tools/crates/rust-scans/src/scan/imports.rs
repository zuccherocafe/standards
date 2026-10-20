//! A file's `use` items: each name they bring into the file and the path it stands for, so a path written through an
//! import resolves to its source.
//!
//! This approximates Rust's scopes file-wide: every `use` in the file binds its name for the whole file, whichever
//! module or block it sits in, and a name bound by several resolves to each. So an `Instant` imported in one inner
//! module resolves an unrelated outer `struct Instant` too, and shadowing by a local item is not tracked. A glob
//! brings in no names. An import's own first segment resolves through the others, so `use std::time; use
//! time::Instant;` binds `Instant` to `std::time::Instant`, through at most [`DEPTH`] imports in a chain.

use std::collections::BTreeMap;

use proc_macro2::Span;
use syn::ext::IdentExt;
use syn::visit::{self, Visit};
use syn::{UseTree, spanned::Spanned};

/// A path as its segments' names.
pub(super) type Segments = Vec<String>;

/// The deepest chain of imports resolved through one another; a longer chain is left part-resolved.
const DEPTH: usize = 8;

/// One leaf of a `use` tree: the path it imports, where it is written, and the name it binds, if any.
#[derive(Debug)]
pub(super) struct Leaf {
    pub(super) path: Segments,
    pub(super) span: Span,
    pub(super) import: Import,
}

/// What a leaf brings into scope.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Import {
    /// The path under a name: its own last segment's, or the one `as` gives.
    Name(String),
    /// Everything the path holds (`*`), by names this scan does not know.
    Glob,
    /// Nothing nameable, as with `as _`.
    Hidden,
}

/// The leaves of one `use` tree, each with its full path.
pub(super) fn leaves(tree: &UseTree) -> Vec<Leaf> {
    let mut found = Vec::new();
    gather(tree, &mut Vec::new(), &mut found);
    found
}

fn gather(tree: &UseTree, prefix: &mut Segments, found: &mut Vec<Leaf>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.unraw().to_string());
            gather(&path.tree, prefix, found);
            prefix.pop();
        }
        UseTree::Name(name) => found.push(named(
            prefix,
            name.ident.unraw().to_string(),
            None,
            name.ident.span(),
        )),
        UseTree::Rename(rename) => {
            let alias = rename.rename.unraw().to_string();
            found.push(named(
                prefix,
                rename.ident.unraw().to_string(),
                Some(alias),
                rename.span(),
            ));
        }
        UseTree::Glob(glob) => found.push(Leaf {
            path: prefix.clone(),
            span: glob.star_token.span,
            import: Import::Glob,
        }),
        UseTree::Group(group) => {
            for tree in &group.items {
                gather(tree, prefix, found);
            }
        }
    }
}

/// A leaf naming `ident` under `prefix`, bound as `alias` or its own name; `self` names the prefix itself.
fn named(prefix: &[String], ident: String, alias: Option<String>, span: Span) -> Leaf {
    let mut path = prefix.to_vec();
    if ident != "self" {
        path.push(ident);
    }
    let import = match alias {
        Some(alias) if alias == "_" => Import::Hidden,
        Some(alias) => Import::Name(alias),
        None => path
            .last()
            .map_or(Import::Hidden, |last| Import::Name(last.clone())),
    };
    Leaf { path, span, import }
}

/// The names a file's imports bind, each to the paths it stands for.
#[derive(Debug, Default)]
pub(super) struct Aliases {
    bound: BTreeMap<String, Vec<Segments>>,
}

impl Aliases {
    /// Every name the `use` items anywhere in `file` bind.
    pub(super) fn of(file: &syn::File) -> Self {
        let mut aliases = Self::default();
        aliases.visit_file(file);
        aliases
    }

    /// Each path `path` may stand for: itself when its first segment is no imported name, otherwise that segment
    /// replaced by each path the name is bound to, resolved in turn.
    pub(super) fn resolve(&self, path: &[String]) -> Vec<Segments> {
        self.expand(path.to_vec(), DEPTH)
    }

    fn expand(&self, path: Segments, depth: usize) -> Vec<Segments> {
        let Some(targets) = path.first().and_then(|first| self.bound.get(first)) else {
            return vec![path];
        };
        if depth == 0 {
            return vec![path];
        }
        targets
            .iter()
            .flat_map(|target| {
                let mut whole = target.clone();
                whole.extend_from_slice(&path[1..]);
                self.expand(whole, depth - 1)
            })
            .collect()
    }
}

impl<'ast> Visit<'ast> for Aliases {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        for leaf in leaves(&item.tree) {
            if let Import::Name(name) = leaf.import {
                let bound = self.bound.entry(name).or_default();
                if !bound.contains(&leaf.path) {
                    bound.push(leaf.path);
                }
            }
        }
        visit::visit_item_use(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(text: &str) -> Segments {
        text.split("::").map(str::to_owned).collect()
    }

    fn aliases(source: &str) -> Aliases {
        Aliases::of(&syn::parse_file(source).expect("made-up source parses"))
    }

    #[test]
    fn a_tree_gives_each_leaf_its_full_path_and_binding() {
        let item: syn::ItemUse =
            syn::parse_str("use std::{fs as files, env, time::*, io::{self}, net as _};")
                .expect("a use item");
        let found: Vec<(String, Import)> = leaves(&item.tree)
            .into_iter()
            .map(|leaf| (leaf.path.join("::"), leaf.import))
            .collect();
        assert_eq!(
            found,
            [
                ("std::fs".to_owned(), Import::Name("files".to_owned())),
                ("std::env".to_owned(), Import::Name("env".to_owned())),
                ("std::time".to_owned(), Import::Glob),
                ("std::io".to_owned(), Import::Name("io".to_owned())),
                ("std::net".to_owned(), Import::Hidden),
            ]
        );
    }

    #[test]
    fn a_renamed_or_grouped_import_resolves_to_its_source() {
        let aliases = aliases("use std::fs as files; fn f() { use std::{env as vars}; }");
        assert_eq!(
            aliases.resolve(&segments("files::read")),
            [segments("std::fs::read")]
        );
        assert_eq!(
            aliases.resolve(&segments("vars::var")),
            [segments("std::env::var")]
        );
    }

    #[test]
    fn a_path_through_no_import_is_itself() {
        let aliases = aliases("use std::fs as files;");
        assert_eq!(
            aliases.resolve(&segments("fs::read")),
            [segments("fs::read")]
        );
        assert_eq!(
            aliases.resolve(&segments("crate::files::read")),
            [segments("crate::files::read")]
        );
    }

    #[test]
    fn an_import_through_another_import_resolves_to_the_source_of_both() {
        let aliases = aliases("use std::time; use time::Instant;");
        assert_eq!(
            aliases.resolve(&segments("Instant::now")),
            [segments("std::time::Instant::now")]
        );
    }

    #[test]
    fn a_name_bound_twice_resolves_to_both() {
        let aliases = aliases("mod a { use std::fs as io; } mod b { use crate::stream as io; }");
        assert_eq!(
            aliases.resolve(&segments("io::x")),
            [segments("std::fs::x"), segments("crate::stream::x")]
        );
    }

    #[test]
    fn an_import_that_names_itself_ends() {
        let aliases = aliases("use a::b as a;");
        assert_eq!(aliases.resolve(&segments("a")).len(), 1);
    }
}
