# Skin design read

- Area: published cut meshes, skeletal skinning, spring solving, rigid accessories and derived offline geometry
- Last full read: ba934cb0b28cbc50107d4206f20144c3f37dd61b
- Inspected: `src/character.rs` Character; `src/mesh.rs` Surface::posed, Model::worlds; `src/coat.rs` Coat::frame; `src/attachments.rs` bind, follow; `src/lib.rs` Body::frame, input_digest; `crates/tdoll-springs/src/solver.rs` Springs::capture, Springs::advance, Springs::drawn

## Models in play

- The character declares piece intent by bone name and target surface by material name. The cut mesh owns vertex indices, inverse binds and weights; one material may name several primitives. A piece is the triangles carrying positive weight on its dedicated bone.
- `Surface::posed` uses glTF linear blending of bone worlds times inverse binds. Weights can stretch and shear a surface. `Model::worlds` owns the parent-first skeleton walk.
- `Coat` captures animated worlds, steps the genuine XPBD solver and writes its drawn rotations before the final skin is evaluated. The spring solver owns its bones, and `body_bones` records explicit body ownership.
- `input_digest` versions a SHA-256 envelope for derived geometry. Model bytes alone describe no authored relationship between a dedicated accessory bone and nearby clothing.

## Overlaps

- The surface skin and accessory bone can share an apparent rest location without sharing motion. Reweighting a badge to clothing bones makes it stretch like cloth; parenting it to a body bone ignores springs beneath it.
- Final offline geometry and support discovery must use the same post-cut primitives, joint-slot lookup and inverse binds. Source indices cannot identify a cut support.

## Half-done transitions

- Character attachment intent and a durable support record have boundary types, but binding refuses attachments and the offline follower does nothing. No engine-side attachment implementation is included in this extraction.

## What later work needs

- Bind a non-degenerate rest support and rigid offset, refuse shared ownership and place the dedicated piece bone after solvers. Keep its authored rest placement and propagate its children before final skinning.
- Include attachment intent and compiled supports in the derived input envelope, so a relationship-only edit cannot reuse unrelated cached geometry.

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| Rigid accessory motion | offline skin | `src/attachments.rs` bind, follow; `src/mesh.rs` Surface::posed; `src/lib.rs` Body::frame, input_digest | A name card stays on its independent bone, while weighting it to the clothing would stretch it like cloth and leave derived caches unaware of the relationship. | A dedicated bone follows a validated deformed triangle rigidly after every solver, rest placement agrees, and attachment changes invalidate caches. |
