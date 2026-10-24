# Interpolation format specifiers

Extend the Soppo compiler in `/app` (`soppo::compile_source(&str)` and `soppo::format_source(&str)`) so interpolated strings support `{expr:spec}` format specifiers.

## Requirements

- Support integer verbs (`d`, `b`, `o`, `x`, `X`, `c`, `U` with optional `+` or space sign, `#`, `0` padding with width, and decimal width), float verbs (`f`, `F`, `e`, `E`, `g`, `G` with optional sign, width, and `.precision`), string verbs (`s`, `q`, `x`, `X`), boolean verb (`t`), and general verb (`v`, `+v`, `#v`).
- Treat a colon at nesting depth zero inside `{...}` as the format delimiter while preserving slice expressions such as `"{arr[1:3]}"` and `"{arr[0:2]:v}"`.
- Reject malformed or unknown directives and type-incompatible directives (identifying the directive and expression type in diagnostics), emit the corresponding `fmt.Sprintf` format string in Go codegen (escaping literal `%` as `%%`), and preserve directives in `src/fmt.rs`.
- This is an unattended run with no interactive user replies; use your engineering judgement for any unstated implementation details and implement, test, and commit your complete change on the working branch in `/app`.
