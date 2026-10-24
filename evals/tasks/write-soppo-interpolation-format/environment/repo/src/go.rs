//! In-memory Go signatures for the isolated compiler fixture.
//!
//! The records and type conversion follow the upstream extraction contract; no
//! resolver, source-file reader or Go subprocess is needed for this task.

mod types;
use std::collections::HashMap;
use std::path::PathBuf;
pub use types::parse_go_type;

#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone)]
pub struct FuncDef {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: String,
    pub location: Option<SourceLocation>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: String,
    pub nullable: bool,
}

#[derive(Debug, Clone)]
pub struct TypeDef {
    pub name: String,
    pub generics: Vec<String>,
    pub kind: String,
    pub fields: Vec<Field>,
    pub underlying: Option<String>,
    pub location: Option<SourceLocation>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConstDef {
    pub ty: String,
    pub location: Option<SourceLocation>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VarDef {
    pub ty: String,
    pub location: Option<SourceLocation>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MethodDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
    pub location: Option<SourceLocation>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SoppoTypeDef {
    pub kind: String,
    pub name: String,
    pub generics: Vec<String>,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Default, Clone)]
pub struct GoPackage {
    pub soppo_generated: bool,
    pub functions: HashMap<String, FuncDef>,
    pub types: HashMap<String, TypeDef>,
    pub constants: HashMap<String, ConstDef>,
    pub variables: HashMap<String, VarDef>,
    pub methods: HashMap<String, Vec<MethodDef>>,
    pub soppo_types: HashMap<String, SoppoTypeDef>,
}

pub struct Project {
    pub root: PathBuf,
    pub module_path: String,
}

pub struct GoCache {
    fmt: GoPackage,
    io: GoPackage,
}

impl GoCache {
    pub fn new() -> miette::Result<Self> {
        let mut fmt = GoPackage::default();
        for name in [
            "Print", "Println", "Printf", "Sprint", "Sprintln", "Sprintf",
        ] {
            let formatted = name.ends_with('f');
            let mut params = Vec::new();
            if formatted {
                params.push(Param {
                    name: "format".into(),
                    ty: "string".into(),
                });
            }
            params.push(Param {
                name: "args".into(),
                ty: "...any".into(),
            });
            let return_type = if name.starts_with('S') {
                "string"
            } else {
                "(int, error)"
            };
            fmt.functions.insert(
                name.into(),
                FuncDef {
                    name: name.into(),
                    generics: Vec::new(),
                    params,
                    return_type: return_type.into(),
                    location: None,
                    doc_comment: None,
                },
            );
        }
        let mut io = GoPackage::default();
        for name in ["Reader", "Writer"] {
            io.types.insert(
                name.into(),
                TypeDef {
                    name: name.into(),
                    generics: Vec::new(),
                    kind: "interface".into(),
                    fields: Vec::new(),
                    underlying: None,
                    location: None,
                    doc_comment: None,
                },
            );
        }
        Ok(Self { fmt, io })
    }

    pub fn get_or_parse(
        &mut self,
        path: &str,
        _project: Option<&Project>,
    ) -> miette::Result<&GoPackage> {
        if path == "fmt" {
            Ok(&self.fmt)
        } else if path == "io" {
            Ok(&self.io)
        } else {
            Err(miette::miette!(
                "package `{path}` is outside the fixture's in-memory catalogue"
            ))
        }
    }
}
