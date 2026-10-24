# Soppo single-source compiler

This fixture extracts the parser, type inference, Go generator and source formatter from halcyonnouveau/soppo commit `686ef0fc5c1ccb71f50d9c65ae9c8d5e2b855e8a`. The BSD licence is in `LICENSE`.

`compile_source(&str)` parses and checks one Soppo file and returns generated Go source. `format_source(&str)` returns formatted Soppo source. Neither entry point invokes Go, reads files or accesses the network. Run `cargo test --offline` to exercise the compiler.

The in-memory import catalogue in `src/go.rs` includes printing functions from `fmt` and the `io.Reader` and `io.Writer` interfaces. Project discovery, filesystem imports, the command-line driver, language servers and external Go package extraction are outside this fixture. The formatter implementation is in `src/fmt.rs`, exported as `soppo::format` and `soppo::format_source`.
