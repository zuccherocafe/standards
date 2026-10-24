//! Typecheck return paths and carry nil facts through terminating branches.

use std::collections::BTreeSet;

use crate::ast::{Expr, Function, TypedStmtKind, block_diverges};
use crate::parser::{ParseError, parse};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Diagnostic {
    MissingReturn,
    NilDereference { binding: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub function: String,
    pub diverges: bool,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn stmt_diverges(statement: &TypedStmtKind) -> bool {
    statement.diverges()
}

fn optional_binding(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Integer(_) => None,
        Expr::TextLength(binding) => Some(binding),
    }
}

fn check_expr(expr: &Expr, nullable: &BTreeSet<String>, diagnostics: &mut Vec<Diagnostic>) {
    let Some(binding) = optional_binding(expr) else {
        return;
    };
    if nullable.contains(binding) {
        diagnostics.push(Diagnostic::NilDereference {
            binding: binding.to_owned(),
        });
    }
}

fn check_block(
    body: &[TypedStmtKind],
    nullable: &mut BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in body {
        match statement {
            TypedStmtKind::Return(expr) => check_expr(expr, nullable, diagnostics),
            TypedStmtKind::Match { arms, .. } => {
                for arm in arms {
                    check_block(&arm.body, &mut nullable.clone(), diagnostics);
                }
            }
            TypedStmtKind::IfNil { binding, body } => {
                check_block(body, &mut nullable.clone(), diagnostics);
                if block_diverges(body) {
                    nullable.remove(binding);
                }
            }
        }
        if stmt_diverges(statement) {
            break;
        }
    }
}

pub fn check_function(function: &Function) -> Report {
    let mut diagnostics = Vec::new();
    let mut nullable = function.nullable_text.iter().cloned().collect();
    check_block(&function.body, &mut nullable, &mut diagnostics);
    let diverges = block_diverges(&function.body);
    if !diverges {
        diagnostics.push(Diagnostic::MissingReturn);
    }
    Report {
        function: function.name.clone(),
        diverges,
        diagnostics,
    }
}

pub fn check_source(source: &str) -> Result<Report, ParseError> {
    Ok(check_function(&parse(source)?))
}
