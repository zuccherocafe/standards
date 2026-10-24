# Interpolation design read

- Area: interpolated strings from Soppo source through type inference, Go generation and source formatting
- Last full read: 686ef0fc5c1ccb71f50d9c65ae9c8d5e2b855e8a
- Inspected: src/syntax/ast.rs, src/syntax/parser/expr.rs, src/types/infer/expr.rs, src/codegen/expr.rs, src/fmt.rs and src/lib.rs

## Models in play

`StringPart` in syntax holds literal segments or expressions. The parser extracts expressions inside braces. Inference checks each expression, code generation emits `%v` for it, and the formatter reconstructs the source interpolation.

## Overlaps

The code generator owns the implicit Go directive while syntax owns the expression. The source formatter has no directive representation to preserve. Explicit directives need a shared meaning across these consumers.

## Half-done transitions

Ordinary interpolation is complete. Explicit format specifiers and their type compatibility rules have no representation or implementation.

## What later work needs

The planned directive feature needs a syntax representation carried through the compiler, type compatibility diagnostics, Go directive rendering and source round trips. Colons in slices and quoted expressions must remain expression syntax.

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| Interpolation format specifiers and type-checked directives | Syntax and type inference | `src/syntax/ast.rs` `StringPart`; `src/syntax/parser/expr.rs` `parse_string_interpolation`; `src/types/infer/expr.rs`; `src/codegen/expr.rs`; `src/fmt.rs` | Directive syntax has no shared owner, so type checking, Go rendering and source formatting cannot agree on explicit directives. | Directives have one checked representation, each consumer uses it, compatibility failures name the directive and type, and public round-trip scenarios pass. |
