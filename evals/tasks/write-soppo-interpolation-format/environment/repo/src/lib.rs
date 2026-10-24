//! Compile and format Soppo source without external tools.
//!
//! The fixture extracts halcyonnouveau/soppo at
//! `686ef0fc5c1ccb71f50d9c65ae9c8d5e2b855e8a`, retaining its parser, type inference,
//! Go emitter and formatter. Its Go package catalogue contains `fmt` calls and
//! `io` interface signatures. Project discovery and external Go imports are
//! outside the single-source API; compilation performs no input or output.
//!
//! ```
//! let go = soppo::compile_source("package main\nfunc main() { println(\"hello\") }").unwrap();
//! assert!(go.contains("println(\"hello\")"));
//! ```

#![allow(clippy::result_large_err)]

pub mod codegen;
mod deps;
pub mod error;
#[path = "fmt.rs"]
pub mod format;
pub mod go;
pub mod syntax;
pub mod types;

pub use format::format_source;

/// Parse, type-check and emit a single Soppo source file as Go source.
///
/// # Errors
/// Returns syntax or type diagnostics, including unknown imports and unused names.
pub fn compile_source(source: &str) -> miette::Result<String> {
    use syntax::{Decl, FileId, Parser};
    let file = Parser::new(source, FileId(0)).parse_file()?;
    let mut infer = types::Infer::new()?;
    infer.process_imports(&file.imports);
    for decl in &file.decls {
        let result = match decl {
            Decl::Const(decl) => infer.infer_const_decl(decl),
            Decl::ConstBlock(decls) => decls
                .iter()
                .try_for_each(|decl| infer.infer_const_decl(decl)),
            Decl::Type(decl) => infer.infer_type_decl(decl),
            Decl::Var(decl) => infer.infer_var_decl(decl),
            Decl::Func(decl) => infer.register_func_signature(decl),
        };
        if let Err(error) = result {
            infer.emit_error(error);
        }
    }
    for decl in &file.decls {
        if let Decl::Func(decl) = decl
            && let Err(error) = infer.infer_func_decl(decl)
        {
            infer.emit_error(error);
        }
    }
    if let Err(error) = infer.check_unused_imports() {
        infer.emit_error(error);
    }
    if let Some(errors) = error::SoppoErrors::new(infer.take_errors()) {
        return Err(errors.into());
    }
    let mut codegen = codegen::Codegen::with_global_state(infer.into_global_state());
    codegen.gen_file(&file)?;
    Ok(codegen.output().to_owned())
}
