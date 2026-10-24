//! A `flake.lock`'s pins: the direct inputs of its root node, each dated by its `locked.lastModified` as a UTC day.

use std::path::Path;

use serde_json::{Map, Value};

use crate::date::Date;
use crate::error::{Cause, FieldProblem, PinError, read_text};
use crate::pins::Pin;

/// One pin for each direct input of the lock's root node that carries a date, in the lock's order. An input that
/// follows another is that other input's pin, and a node without `lastModified`, such as a path input, has no date;
/// both are skipped. Refused for a file that cannot be read or parsed, or a lock whose structure or times are
/// malformed: a present `lastModified` must be an integer within the calendar.
pub(crate) fn lock_pins(path: &Path) -> Result<Vec<Pin>, PinError> {
    let text = read_text(path)?;
    let lock: Value = serde_json::from_str(&text)
        .map_err(|error| PinError::new(path, Cause::Json(error.to_string())))?;
    let Value::Object(lock) = lock else {
        return Err(PinError::new(path, Cause::NotAJsonObject));
    };
    let Some(Value::Object(nodes)) = lock.get("nodes") else {
        return Err(PinError::field(path, "nodes", FieldProblem::NotAnObject));
    };
    let Some(Value::String(root_name)) = lock.get("root") else {
        return Err(PinError::field(path, "root", FieldProblem::NotAString));
    };
    let Some(Value::Object(root)) = nodes.get(root_name) else {
        return Err(PinError::field(
            path,
            format!("nodes.{root_name}"),
            FieldProblem::NoRootNode,
        ));
    };
    let no_inputs = Map::new();
    let inputs = match root.get("inputs") {
        None => &no_inputs,
        Some(Value::Object(inputs)) => inputs,
        Some(_) => {
            return Err(PinError::field(
                path,
                format!("nodes.{root_name}.inputs"),
                FieldProblem::PresentButNotAnObject,
            ));
        }
    };
    let mut pins = Vec::new();
    for (name, reference) in inputs {
        let input = || format!("nodes.{root_name}.inputs.{name}");
        let reference = match reference {
            Value::Array(_) => continue,
            Value::String(reference) => reference,
            _ => return Err(PinError::field(path, input(), FieldProblem::NotAReference)),
        };
        let Some(Value::Object(node)) = nodes.get(reference) else {
            return Err(PinError::field(
                path,
                input(),
                FieldProblem::NoSuchNode(reference.clone()),
            ));
        };
        let Some(Value::Object(locked)) = node.get("locked") else {
            return Err(PinError::field(
                path,
                format!("nodes.{reference}.locked"),
                FieldProblem::NotAnObject,
            ));
        };
        let Some(modified) = locked.get("lastModified") else {
            continue;
        };
        let field = || format!("nodes.{reference}.locked.lastModified");
        let Some(seconds) = whole_seconds(modified) else {
            return Err(PinError::field(
                path,
                field(),
                FieldProblem::NotAnInteger(Box::new(modified.clone())),
            ));
        };
        let published = Date::from_unix_seconds(seconds)
            .ok_or_else(|| PinError::field(path, field(), FieldProblem::NotATime(seconds)))?;
        pins.push(Pin::new(
            format!("flake input {name} in {}", path.display()),
            published,
        ));
    }
    Ok(pins)
}

/// The value as a whole number, when JSON reads it as one within 64 bits, signed or not.
fn whole_seconds(value: &Value) -> Option<i128> {
    let number = value.as_number()?;
    number
        .as_i64()
        .map(i128::from)
        .or_else(|| number.as_u64().map(i128::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Scratch, lock, node};

    /// 2023-11-14 22:13:20 UTC, and the first second of 2023-11-15 UTC.
    const LATE_ON_THE_14TH: &str = "1700000000";
    const START_OF_THE_15TH: &str = "1700006400";

    fn day(year: u16, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("a real day")
    }

    #[test]
    fn each_input_of_the_named_root_is_dated_by_its_last_modification_in_utc_in_the_locks_order() {
        let folder = Scratch::new("inputs");
        let path = folder.write(
            "flake.lock",
            &lock(
                "top",
                r#"{"overlay": "overlay", "nixpkgs": "nixpkgs_2"}"#,
                &[
                    ("nixpkgs_2", &node(LATE_ON_THE_14TH)),
                    ("overlay", &node(START_OF_THE_15TH)),
                ],
            ),
        );
        let shown = path.display();
        assert_eq!(
            lock_pins(&path).expect("read"),
            [
                Pin::new(format!("flake input overlay in {shown}"), day(2023, 11, 15)),
                Pin::new(format!("flake input nixpkgs in {shown}"), day(2023, 11, 14)),
            ]
        );
    }

    #[test]
    fn an_input_that_follows_another_and_an_undated_input_are_skipped() {
        let folder = Scratch::new("skipped");
        let path = folder.write(
            "flake.lock",
            &lock(
                "root",
                r#"{"nixpkgs": "nixpkgs", "follower": ["nixpkgs"], "local": "local"}"#,
                &[
                    ("nixpkgs", &node(LATE_ON_THE_14TH)),
                    (
                        "local",
                        r#"{"locked": {"path": "./local", "type": "path"}}"#,
                    ),
                ],
            ),
        );
        assert_eq!(
            lock_pins(&path).expect("read"),
            [Pin::new(
                format!("flake input nixpkgs in {}", path.display()),
                day(2023, 11, 14)
            )]
        );
    }

    #[test]
    fn a_root_without_inputs_has_no_pins() {
        let folder = Scratch::new("no-inputs");
        for text in [
            lock("root", "{}", &[]),
            r#"{"nodes": {"root": {}}, "root": "root", "version": 7}"#.to_owned(),
        ] {
            let path = folder.write("flake.lock", &text);
            assert_eq!(lock_pins(&path).expect("read"), [], "{text}");
        }
    }

    #[test]
    fn a_malformed_lock_is_refused_naming_the_field() {
        let folder = Scratch::new("malformed");
        let one = |node: &str| lock("root", r#"{"nixpkgs": "nixpkgs"}"#, &[("nixpkgs", node)]);
        for (case, text, message) in [
            ("not an object", "[]".to_owned(), "is not a JSON object"),
            (
                "no nodes",
                r#"{"root": "root"}"#.to_owned(),
                "nodes: missing, or not an object",
            ),
            (
                "no root",
                r#"{"nodes": {}, "version": 7}"#.to_owned(),
                "root: missing, or not a string",
            ),
            (
                "a missing root node",
                r#"{"nodes": {}, "root": "root"}"#.to_owned(),
                "nodes.root: the root node is missing, or not an object",
            ),
            (
                "inputs that are not an object",
                r#"{"nodes": {"root": {"inputs": []}}, "root": "root"}"#.to_owned(),
                "nodes.root.inputs: not an object",
            ),
            (
                "an input that is neither",
                lock("root", r#"{"nixpkgs": 7}"#, &[]),
                "nodes.root.inputs.nixpkgs: neither a node name nor a follows path",
            ),
            (
                "a missing input node",
                lock("root", r#"{"nixpkgs": "gone"}"#, &[]),
                "nodes.root.inputs.nixpkgs: names node \"gone\", which is missing",
            ),
            (
                "a text time",
                one(&node(r#""1700000000""#)),
                "nodes.nixpkgs.locked.lastModified: \"1700000000\" is not an integer",
            ),
            (
                "a null time",
                one(&node("null")),
                "nodes.nixpkgs.locked.lastModified: null is not an integer",
            ),
            (
                "a boolean time",
                one(&node("true")),
                "nodes.nixpkgs.locked.lastModified: true is not an integer",
            ),
            (
                "a fractional time",
                one(&node("1.5")),
                "nodes.nixpkgs.locked.lastModified: 1.5 is not an integer",
            ),
            (
                "a time past the calendar",
                one(&node("253402300800")),
                "nodes.nixpkgs.locked.lastModified: 253402300800 is not a time",
            ),
            (
                "a node without locked",
                one("{}"),
                "nodes.nixpkgs.locked: missing, or not an object",
            ),
        ] {
            let path = folder.write("flake.lock", &text);
            assert_eq!(
                lock_pins(&path).expect_err(case).to_string(),
                format!("{}: {message}", path.display()),
                "{case}"
            );
        }
    }

    #[test]
    fn a_file_that_is_not_json_is_refused() {
        let folder = Scratch::new("not-json");
        let path = folder.write("flake.lock", "{");
        let refusal = lock_pins(&path).expect_err("refused").to_string();
        assert!(
            refusal.starts_with(&format!("{}: is not JSON: ", path.display())),
            "{refusal}"
        );
    }
}
