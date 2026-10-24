//! Typed control flow shared by return checking and nil narrowing.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateVariant {
    Open,
    Closed,
}

impl GateVariant {
    pub const ALL: [Self; 2] = [Self::Open, Self::Closed];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Integer(i64),
    TextLength(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    pub variant: GateVariant,
    pub guard: Option<String>,
    pub body: Vec<TypedStmtKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedStmtKind {
    Return(Expr),
    Match {
        scrutinee: String,
        arms: Vec<MatchArm>,
    },
    IfNil {
        binding: String,
        body: Vec<Self>,
    },
}

impl TypedStmtKind {
    pub fn diverges(&self) -> bool {
        match self {
            Self::Return(_) => true,
            Self::IfNil { .. } => false,
            Self::Match { .. } => false,
        }
    }
}

pub fn block_diverges(body: &[TypedStmtKind]) -> bool {
    body.iter().any(TypedStmtKind::diverges)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub nullable_text: Vec<String>,
    pub body: Vec<TypedStmtKind>,
}
