# Offline skin and spring conversion

This is a focused extraction of `zuccherocafe/tdollhouse` at `ba934cb0b28cbc50107d4206f20144c3f37dd61b`. It has no engine or model-import dependency. Run `cargo test --locked --offline --workspace` from this directory.

`crates/tdoll-vocab` preserves the upstream domain vocabulary, including `BoneName`, `Length` and `Seconds`. `crates/tdoll-springs` preserves the genuine XPBD chain solver, limits, collider and contact algorithms. Their MIT licences are included. Only Kani declarations, proof files and standalone tests needing allocation/property-test dependencies are excluded.

`src/mesh.rs` adapts `tdoll-convert/src/retarget/body/mesh.rs`: a surface keeps its held positions, positive joint weights, inverse binds and post-cut triangles, and skins them by the same linear-blend equation. The fixture uses vectors with checked indices instead of the importer's typed index containers and takes decoded cut primitives instead of parsing glTF. `MaterialName` is a fixture boundary type; upstream identifies materials with strings. A piece is every triangle carrying positive weight on its named bone. A rigid piece must be wholly weighted to that one dedicated bone. A target material can span several cut primitives.

`src/coat.rs` adapts `tdoll-convert-driver/src/body/coat.rs`: capture the clip pose, step the genuine solver, write its drawn local rotations and walk parent-first worlds again. `SpringChain` exposes the chain feel needed here; it constructs upstream chain specifications without furniture or joint boxes. `src/lib.rs` owns completed offline skinning and the SHA-256 derived-input envelope, corresponding to `stages/hang.rs` inputs. The attachment declarations and record are prepared API boundaries; support binding and following are not implemented.

The character reader accepts a focused line-based subset: `character {`, `attach "Badge" to="Cloth"`, and `}`. It also accepts bare attachment statements for callers constructing a character fragment. Names are quoted and contain no escapes. No KDL or binary model importer is needed to exercise the conversion contracts.

The area read is [Skin](docs/design-reads/skin.md).
