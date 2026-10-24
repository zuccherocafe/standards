//! Parse the executable enum-flow language slice; no filesystem access belongs here.

use crate::ast::{Expr, Function, GateVariant, MatchArm, TypedStmtKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub token: usize,
    pub expected: &'static str,
}

struct Parser {
    tokens: Vec<String>,
    position: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.position).map(String::as_str)
    }

    fn take(&mut self) -> Result<String, ParseError> {
        let token = self
            .tokens
            .get(self.position)
            .cloned()
            .ok_or(self.error("token"))?;
        self.position += 1;
        Ok(token)
    }

    fn error(&self, expected: &'static str) -> ParseError {
        ParseError {
            token: self.position,
            expected,
        }
    }

    fn expect(&mut self, token: &'static str) -> Result<(), ParseError> {
        if self.peek() != Some(token) {
            return Err(self.error(token));
        }
        self.position += 1;
        Ok(())
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.take()?;
        if let Ok(number) = token.parse() {
            return Ok(Expr::Integer(number));
        }
        self.expect(".")?;
        self.expect("length")?;
        Ok(Expr::TextLength(token))
    }

    fn block(&mut self) -> Result<Vec<TypedStmtKind>, ParseError> {
        self.expect("{")?;
        let mut body = Vec::new();
        while self.peek() != Some("}") {
            body.push(self.stmt()?);
        }
        self.expect("}")?;
        Ok(body)
    }

    fn stmt(&mut self) -> Result<TypedStmtKind, ParseError> {
        match self.take()?.as_str() {
            "return" => {
                let expr = self.expr()?;
                self.expect(";")?;
                Ok(TypedStmtKind::Return(expr))
            }
            "if" => {
                let binding = self.take()?;
                self.expect("is")?;
                self.expect("nil")?;
                let body = self.block()?;
                Ok(TypedStmtKind::IfNil { binding, body })
            }
            "match" => {
                let scrutinee = self.take()?;
                self.expect("{")?;
                let mut arms = Vec::new();
                while self.peek() != Some("}") {
                    let variant = match self.take()?.as_str() {
                        "Open" => GateVariant::Open,
                        "Closed" => GateVariant::Closed,
                        _ => return Err(self.error("Gate variant")),
                    };
                    let guard = if self.peek() == Some("if") {
                        self.take()?;
                        Some(self.take()?)
                    } else {
                        None
                    };
                    self.expect("=")?;
                    self.expect(">")?;
                    let body = self.block()?;
                    arms.push(MatchArm {
                        variant,
                        guard,
                        body,
                    });
                    if self.peek() == Some(",") {
                        self.take()?;
                    }
                }
                self.expect("}")?;
                Ok(TypedStmtKind::Match { scrutinee, arms })
            }
            _ => Err(self.error("return, if or match")),
        }
    }

    fn function(&mut self) -> Result<Function, ParseError> {
        self.expect("fn")?;
        let name = self.take()?;
        self.expect("(")?;
        let mut nullable_text = Vec::new();
        while self.peek() != Some(")") {
            let binding = self.take()?;
            self.expect(":")?;
            let kind = self.take()?;
            match kind.as_str() {
                "Text" => {
                    if self.peek() == Some("?") {
                        self.take()?;
                        nullable_text.push(binding);
                    }
                }
                "Gate" | "bool" => {}
                _ => return Err(self.error("parameter type")),
            }
            if self.peek() != Some(")") {
                self.expect(",")?;
            }
        }
        self.expect(")")?;
        self.expect("-")?;
        self.expect(">")?;
        self.expect("int")?;
        let body = self.block()?;
        if self.peek().is_some() {
            return Err(self.error("end of file"));
        }
        Ok(Function {
            name,
            nullable_text,
            body,
        })
    }
}

pub fn parse(source: &str) -> Result<Function, ParseError> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    for character in source.chars() {
        if character.is_alphanumeric() || character == '_' {
            word.push(character);
        } else {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            if !character.is_whitespace() {
                tokens.push(character.to_string());
            }
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    Parser {
        tokens,
        position: 0,
    }
    .function()
}
