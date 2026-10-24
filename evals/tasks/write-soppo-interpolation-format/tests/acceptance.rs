//! Public compiler uses, most common first: compile messages, format source, refuse invalid directives.

use soppo::{compile_source, format_source};

fn program(body: &str) -> String {
    format!("package main\nfunc main() {{\n{body}\n}}\n")
}

fn generated(body: &str) -> String {
    let source = program(body);
    compile_source(&source).unwrap_or_else(|error| panic!("{source}\n{error}"))
}

#[test]
fn compile_mixed_message_with_literal_percent() {
    let go = generated(
        r#"n := 42
price := 19.99
name := "Ada"
flag := true
println("ID {n:04x}; price {price:.2f}; name {name:s}; active {flag:t}; 100% {n}")"#,
    );
    assert!(
        go.contains(r#"fmt.Sprintf("ID %04x; price %.2f; name %s; active %t; 100%% %v", n, price, name, flag, n)"#),
        "{go}"
    );
}

#[test]
fn accept_integer_types_and_verbs() {
    for ty in [
        "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32", "uint64",
        "byte", "rune",
    ] {
        for spec in ["d", "b", "o", "x", "X", "c", "U", "+d", " d", "#x", "04x", "+#08X", "8d"] {
            let go = generated(&format!("var n {ty} = 42\nprintln(\"{{n:{spec}}}\")"));
            assert!(go.contains(&format!("%{spec}")), "{ty}, {spec}: {go}");
        }
    }
}

#[test]
fn accept_float_types_and_verbs() {
    for ty in ["float32", "float64"] {
        for spec in ["f", "F", "e", "E", "g", "G", "+8.2f", " 8.0G", ".2f", "8f", ".0e"] {
            let go = generated(&format!("var n {ty} = 2.5\nprintln(\"{{n:{spec}}}\")"));
            assert!(go.contains(&format!("%{spec}")), "{ty}, {spec}: {go}");
        }
    }
}

#[test]
fn accept_string_bool_and_general_directives() {
    for spec in ["s", "q", "x", "X"] {
        let go = generated(&format!("name := \"hello\"\nprintln(\"{{name:{spec}}}\")"));
        assert!(go.contains(&format!("%{spec}")), "{spec}: {go}");
    }
    for (declaration, expression, spec) in [
        ("flag := true", "flag", "t"),
        ("flag := true", "flag", "v"),
        ("name := \"Ada\"", "name", "v"),
        ("items := []int{1, 2}", "items", "v"),
        ("items := []int{1, 2}", "items", "#v"),
        ("n := 42", "n", "+v"),
    ] {
        let go = generated(&format!("{declaration}\nprintln(\"{{{expression}:{spec}}}\")"));
        assert!(go.contains(&format!("%{spec}")), "{spec}: {go}");
    }
}

#[test]
fn distinguish_slice_colons_from_directives() {
    let go = generated(
        r#"arr := []int{10, 20, 30}
println("val: {arr[1:3]} {arr[0:2]:v} {arr[0]:04x}")"#,
    );
    assert!(go.contains("val: %v %v %04x"), "{go}");
    assert!(go.contains("arr[1:3], arr[0:2], arr[0]"), "{go}");
}

#[test]
fn compile_expressions_with_directives() {
    let go = generated("x := 2\ny := 3\nprintln(\"{x + y:d} {float64(x) * 2.5:.1f}\")");
    assert!(go.contains("%d %.1f"), "{go}");
    assert!(go.contains("x + y"), "{go}");
}

#[test]
fn preserve_directives_in_formatted_source() {
    let source = program(
        r#"arr := []int{1, 2, 3}
f := 2.5
name := "Ada"
flag := true
println("{arr[1:3]} {arr[0:2]:v} {arr[0]:04x} {f:+8.2f} {name:q} {flag:t} 50%")"#,
    );
    let formatted = format_source(&source).unwrap();
    for interpolation in ["{arr[1:3]}", "{arr[0:2]:v}", "{arr[0]:04x}", "{f:+8.2f}", "{name:q}", "{flag:t}"] {
        assert!(formatted.contains(interpolation), "{interpolation}: {formatted}");
    }
    assert_eq!(format_source(&formatted).unwrap(), formatted);
    let go = compile_source(&formatted).unwrap();
    assert!(go.contains("%v %v %04x %+8.2f %q %t 50%%"), "{go}");
}

#[test]
fn report_incompatible_expression_types() {
    for (declaration, expression, spec, ty) in [
        ("n := 42", "n", ".2f", "int"),
        ("", "42", ".2f", "int"),
        ("name := \"hello\"", "name", "d", "string"),
        ("", "\\\"hello\\\"", "d", "string"),
        ("flag := true", "flag", "s", "bool"),
        ("n := 42", "n", "t", "int"),
        ("f := 2.5", "f", "x", "float64"),
        ("items := []int{1, 2}", "items", "d", "[]int"),
    ] {
        let source = program(&format!("{declaration}\nprintln(\"{{{expression}:{spec}}}\")"));
        let error = compile_source(&source).unwrap_err().to_string();
        assert!(error.contains(spec) && error.contains(ty), "{source}: {error}");
    }
}

#[test]
fn refuse_empty_unknown_and_malformed_directives() {
    for spec in [
        "", "z", "++d", "+ d", "##x", "004x", "4-d", "0d", ".f", ".2.3f", "2.f", "4dd",
        "-4d", "*d", "2df", ".2d", "#f", "08f", "+t", ".2s", "999999999999999999999d",
    ] {
        let source = program(&format!("n := 42\nprintln(\"{{n:{spec}}}\")"));
        assert!(compile_source(&source).is_err(), "accepted {spec:?}");
    }
}

#[test]
fn ordinary_interpolation_remains_available() {
    let go = generated("n := 42\nprintln(\"value {n} 10%\")");
    assert!(go.contains("value %v 10%%"), "{go}");
}
