# Web serving design read

- Area: domain-scoped static files, node configuration and image requests
- Last full read: c01ba4110bc9aa06238c96c6d1e3cdb9bef31625
- Inspected: `src/lib.rs`, `src/config.rs`, `src/web.rs`

## Models in play

`Config` owns `WebConfig` and the domain alias map. `WebConfig.static_dir` names the parent of domain directories. `WebServer` resolves the hostname and aliases, confines a canonical path to the selected domain root and returns `Response` bytes and their media type. Requests carry raw query text, which static serving ignores.

## Overlaps

The planned image policy and query options both describe dimensions, formats and quality. Keeping raw numeric configuration fields beside untyped query values would give validation and defaulting two owners. A disk cache would need a representation of the complete transformation rather than only a path and timestamp.

## Half-done transitions

Image processing is not modelled. Static serving has one path resolver and no transformed representation or output cache. The open design choices concern how the image model joins this serving boundary without a second resolver or inconsistent refusals.

## What later work needs

Dynamic PNG transformations need configuration limits, validated query options, geometry and output encoding. Rebuildable cached bytes need an identity that includes source content and all result-shaping choices. Untransformed requests must remain independent of image limits.

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| Image query vocabulary | web serving | `src/web.rs` `WebServer::serve` | Raw query text cannot carry validated fit, format, dimensions or aliases into processing. | Query parsing produces one checked transformation request with closed modes. |
| Image policy quantities | configuration | `src/config.rs` `Config`; `src/web.rs` `WebServer::serve` | Raw unit fields would allow invalid limits and repeat validation at each request. | Configuration and requests share validated quantity types; loading refuses invalid policy. |
| Transformed output identity | web serving | `src/web.rs` `WebServer::serve` | A path or timestamp alone can reuse stale bytes or collide across output choices. | One complete recipe plus source content digest defines each cache entry. |
