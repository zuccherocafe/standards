//! A script's declarations, read with the GDScript grammar: each script-level and inner-class `func`, `var`,
//! `const`, `signal` and inner `class`, with what the scans need of it. A script the grammar cannot parse declares
//! nothing known; [`declarations`] says where the parse failed.

use tree_sitter::{Node, Parser};

/// What a declaration declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Function,
    Variable,
    Constant,
    Signal,
    Class,
}

/// One declaration of a script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Declaration {
    pub(crate) kind: Kind,
    pub(crate) name: String,
    /// `<Class>.<member>`, the class being the script's (or an inner class's path below it).
    pub(crate) symbol: String,
    /// The name's line, counted from 1.
    pub(crate) line: u32,
    /// The name's byte offset, which tells the declaration from the other occurrences of its name.
    pub(crate) offset: usize,
    /// For a variable: whether an `@export` annotation other than `@export_storage` marks it, or it has a setter.
    pub(crate) used_by_engine: bool,
    /// For a function: each bare part of its signature, in order.
    pub(crate) bare: Vec<String>,
}

impl Declaration {
    /// Whether the name is public: no leading underscore.
    pub(crate) fn public(&self) -> bool {
        !self.name.starts_with('_')
    }
}

/// Why a script's declarations are unknown.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Unparsed {
    #[error("the grammar cannot parse it at line {0}")]
    Error(usize),
    #[error("the grammar could not be loaded")]
    Grammar,
}

/// The annotations that group the inspector's fields rather than export a variable.
const GROUPING: [&str; 3] = ["export_group", "export_subgroup", "export_category"];

/// Every declaration of the script `text`, whose class is `class`.
///
/// # Errors
/// [`Unparsed`] when the grammar finds an error in the script, at the first error's line.
pub(crate) fn declarations(text: &str, class: &str) -> Result<Vec<Declaration>, Unparsed> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .map_err(|_| Unparsed::Grammar)?;
    let tree = parser.parse(text, None).ok_or(Unparsed::Grammar)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(Unparsed::Error(
            first_error(root).map_or(1, |node| node.start_position().row + 1),
        ));
    }
    let class = named_class(root, text).unwrap_or(class);
    let mut found = Vec::new();
    members(root, text, class, &mut found);
    Ok(found)
}

/// The script's `class_name`, if it has one.
fn named_class<'a>(root: Node<'_>, text: &'a str) -> Option<&'a str> {
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .find(|child| child.kind() == "class_name_statement")
        .and_then(|statement| statement.child_by_field_name("name"))
        .map(|name| &text[name.byte_range()])
}

fn first_error(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).find_map(first_error)
}

/// The declarations directly in `body`, a script or an inner class's body, and in each inner class below it.
fn members(body: Node<'_>, text: &str, class: &str, found: &mut Vec<Declaration>) {
    let mut cursor = body.walk();
    // Annotations on lines of their own apply to the statement after them.
    let mut pending: Vec<String> = Vec::new();
    for statement in body.named_children(&mut cursor) {
        let kind = match statement.kind() {
            "comment" | "region_start" | "region_end" => continue,
            "annotation" => {
                pending.extend(annotation_name(statement, text));
                continue;
            }
            "function_definition" => Some(Kind::Function),
            "variable_statement" | "export_variable_statement" | "onready_variable_statement" => {
                Some(Kind::Variable)
            }
            "const_statement" => Some(Kind::Constant),
            "signal_statement" => Some(Kind::Signal),
            "class_definition" => Some(Kind::Class),
            _ => None,
        };
        let annotations = std::mem::take(&mut pending);
        let (Some(kind), Some(name)) = (kind, statement.child_by_field_name("name")) else {
            continue;
        };
        let written = &text[name.byte_range()];
        let symbol = format!("{class}.{written}");
        let mut all = annotations;
        all.extend(attached_annotations(statement, text));
        found.push(Declaration {
            kind,
            name: written.to_owned(),
            symbol: symbol.clone(),
            line: u32::try_from(name.start_position().row + 1).unwrap_or(u32::MAX),
            offset: name.start_byte(),
            used_by_engine: kind == Kind::Variable && (exported(&all) || has_setter(statement)),
            bare: if kind == Kind::Function {
                bare_parts(statement, text)
            } else {
                Vec::new()
            },
        });
        if kind == Kind::Class
            && let Some(inner) = statement.child_by_field_name("body")
        {
            members(inner, text, &symbol, found);
        }
    }
}

fn annotation_name(annotation: Node<'_>, text: &str) -> Option<String> {
    let mut cursor = annotation.walk();
    annotation
        .named_children(&mut cursor)
        .find(|child| child.kind() == "identifier")
        .map(|name| text[name.byte_range()].to_owned())
}

/// The annotations written on the statement's own line, before its keyword.
fn attached_annotations(statement: Node<'_>, text: &str) -> Vec<String> {
    let mut cursor = statement.walk();
    let Some(list) = statement
        .named_children(&mut cursor)
        .find(|child| child.kind() == "annotations")
    else {
        return Vec::new();
    };
    let mut inner = list.walk();
    list.named_children(&mut inner)
        .filter_map(|annotation| annotation_name(annotation, text))
        .collect()
}

/// Whether the annotations export the variable to the inspector: `@export` or an `@export_*` form, but not
/// `@export_storage`, which only serialises it, nor the grouping annotations.
fn exported(annotations: &[String]) -> bool {
    annotations.iter().any(|name| {
        name == "export"
            || (name.starts_with("export_")
                && name != "export_storage"
                && !GROUPING.contains(&name.as_str()))
    })
}

fn has_setter(statement: Node<'_>) -> bool {
    statement
        .child_by_field_name("setget")
        .is_some_and(|setget| setget.child_by_field_name("set").is_some())
}

/// The bare parts of a function's signature, in order: each parameter that is untyped or typed `Dictionary`,
/// `Array` or `Variant` with no element type, then the return when it is untyped or one of those. An inferred
/// parameter (`bound := 0.0`) is typed, and a variadic one (`...rest: Array`) can be nothing but an `Array`.
fn bare_parts(function: Node<'_>, text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(parameters) = function.child_by_field_name("parameters") {
        let mut cursor = parameters.walk();
        for parameter in parameters.named_children(&mut cursor) {
            let name = || {
                let mut inner = parameter.walk();
                parameter
                    .named_children(&mut inner)
                    .find(|child| child.kind() == "identifier")
                    .map_or_else(String::new, |name| text[name.byte_range()].to_owned())
            };
            match parameter.kind() {
                "identifier" => parts.push(format!("{} untyped", &text[parameter.byte_range()])),
                "default_parameter" => parts.push(format!("{} untyped", name())),
                "typed_parameter" | "typed_default_parameter" => {
                    if let Some(bare) = parameter
                        .child_by_field_name("type")
                        .and_then(|kind| bare_type(kind, text))
                    {
                        parts.push(format!("{} {bare}", name()));
                    }
                }
                _ => {}
            }
        }
    }
    match function.child_by_field_name("return_type") {
        None => parts.push("returns untyped".to_owned()),
        Some(kind) => {
            if let Some(bare) = bare_type(kind, text) {
                parts.push(format!("returns {bare}"));
            }
        }
    }
    parts
}

/// The type's name when it is bare: `Dictionary` or `Array` with no element type, or `Variant`. An inferred type
/// (`:=`) is not bare.
fn bare_type<'a>(kind: Node<'_>, text: &'a str) -> Option<&'a str> {
    let written = text[kind.byte_range()].trim();
    (kind.kind() == "type" && matches!(written, "Dictionary" | "Array" | "Variant"))
        .then_some(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(text: &str) -> Vec<Declaration> {
        declarations(text, "stem").expect("the script parses")
    }

    fn symbols(text: &str) -> Vec<String> {
        declared(text)
            .into_iter()
            .map(|declaration| declaration.symbol)
            .collect()
    }

    #[test]
    fn every_script_level_member_kind_is_declared_under_the_class_name() {
        let text = "class_name GFThing\nextends Node\nsignal moved\nconst LIMIT := 3\nenum Mode { A }\n\
                    static var count := 0\nvar size := 1.0\n@onready var child: Node = $Child\n\
                    func run() -> void:\n\tvar local := 1\nstatic func make() -> int:\n\treturn 0\n";
        assert_eq!(
            symbols(text),
            [
                "GFThing.moved",
                "GFThing.LIMIT",
                "GFThing.count",
                "GFThing.size",
                "GFThing.child",
                "GFThing.run",
                "GFThing.make"
            ]
        );
    }

    #[test]
    fn a_script_without_a_class_name_is_named_by_what_it_is_given() {
        assert_eq!(symbols("extends Node\nvar size := 1.0\n"), ["stem.size"]);
    }

    #[test]
    fn an_inner_class_and_its_members_are_named_below_their_outer_class() {
        let text = "class_name Outer\nclass Inner extends RefCounted:\n\tvar field: int\n\tclass Deep:\n\
                    \t\tfunc go() -> void:\n\t\t\tpass\n";
        assert_eq!(
            symbols(text),
            [
                "Outer.Inner",
                "Outer.Inner.field",
                "Outer.Inner.Deep",
                "Outer.Inner.Deep.go"
            ]
        );
    }

    #[test]
    fn a_variable_exported_to_the_inspector_or_with_a_setter_is_used_by_the_engine() {
        let text = "@export var a := 1\n@export\n# A comment between.\n#region Fields\nvar b := 1\n#endregion\n\
                    @export_range(0, 1) var c := 0.5\n\
                    @export_storage var d := false\n@export_group(\"Size\")\nvar e := 1\n\
                    var f: int:\n\tset(value):\n\t\tf = value\nvar g: int: get = _get_g\nvar h := 1\n";
        let engine: Vec<(String, bool)> = declared(text)
            .into_iter()
            .map(|declaration| (declaration.name, declaration.used_by_engine))
            .collect();
        let expected = [
            ("a", true),
            ("b", true),
            ("c", true),
            ("d", false),
            ("e", false),
            ("f", true),
            ("g", false),
            ("h", false),
        ];
        assert_eq!(engine, expected.map(|(name, used)| (name.to_owned(), used)));
    }

    #[test]
    fn a_signature_lists_each_untyped_or_unparameterised_part_in_order() {
        let text = "func f(a, b: Array, c := 0.0, d: Variant = null, e = 1, g: Array[int] = [], \
                    h: Dictionary[String, int] = {}) -> Dictionary:\n\tpass\n\
                    func typed(x: int) -> void:\n\tpass\nfunc untyped_return(x: int):\n\tpass\n\
                    func rest(...more: Array) -> void:\n\tpass\n";
        let bare: Vec<Vec<String>> = declared(text)
            .into_iter()
            .map(|declaration| declaration.bare)
            .collect();
        assert_eq!(
            bare,
            [
                vec![
                    "a untyped",
                    "b Array",
                    "d Variant",
                    "e untyped",
                    "returns Dictionary"
                ],
                vec![],
                vec!["returns untyped"],
                vec![],
            ]
        );
    }

    #[test]
    fn a_script_the_grammar_cannot_parse_says_at_which_line() {
        assert_eq!(
            declarations("var fine := 1\nvar broken = (\n", "stem"),
            Err(Unparsed::Error(2))
        );
    }
}
