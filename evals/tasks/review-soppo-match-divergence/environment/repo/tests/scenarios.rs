//! Uses: reject nullable access, check return paths, and narrow after terminating nil branches.

use soppo::infer::{Diagnostic, check_source};

#[test]
fn nullable_text_requires_a_check() {
    let report = check_source("fn size(value: Text?) -> int { return value.length; }").unwrap();
    assert_eq!(
        report.diagnostics,
        vec![Diagnostic::NilDereference {
            binding: "value".into()
        }]
    );
}

#[test]
fn an_empty_function_is_missing_a_return() {
    let report = check_source("fn empty() -> int {}").unwrap();
    assert_eq!(report.diagnostics, vec![Diagnostic::MissingReturn]);
}

#[test]
fn a_nil_return_leaves_only_non_nil_values() {
    let report = check_source(
        "fn size(value: Text?) -> int { if value is nil { return 0; } return value.length; }",
    )
    .unwrap();
    assert!(report.diagnostics.is_empty());
}
