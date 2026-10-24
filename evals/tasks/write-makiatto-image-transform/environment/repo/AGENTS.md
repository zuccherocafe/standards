# Working in this extraction

This crate extracts Makiatto's node configuration, domain lookup and static file serving. Transport, TLS, databases, async tasks and network access are outside its scope. A domain is a subdirectory of `web.static_dir`; `domains` maps alias hostnames to domain directory names. Directory requests serve `index.html`.

`Config::from_toml` and `WebServer::serve` are the configuration and request boundaries. Keep their signatures and the `Response` body and media-type fields stable so an HTTP adapter can use them. Add error variants and configuration fields as needed. Run checks from this directory:

```sh
cargo fmt --all --check
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
```

The cached dependencies and compiler are already installed. This is an isolated working branch; commit the completed change here. No network, service or external image fixtures are needed. Keep scratch data in `.cache/`.
