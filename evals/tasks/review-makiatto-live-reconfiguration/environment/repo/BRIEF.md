# Static transform service

The service owns one HTTP listener, completes one bounded request at a time, and joins its task on shutdown. A configured peer supplies a byte-filter WASM module and invocation environment. `RouteWeight::new` and the configuration boundary both require a positive route weight. An empty peer list serves the document unchanged.

Each transform has a fresh Wasmtime store with instruction fuel. The compiled module's code does not capture the store's environment. This executable slice leaves TLS and full HTTP parsing to its enclosing deployment.
