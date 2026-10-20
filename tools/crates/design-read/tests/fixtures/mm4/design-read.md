# MM4 design read

- Area: motion matching's runtime: the motion database, the body's motion source and the gym's steering
- Revision: 80adaf90f355bb9ead3e0b8de86f0632f72c34d8
- Previous: none
- Inspected: `tdoll-motion`; `stages/motion.rs`, `records/motion.rs`, `explain/database.rs`; `GFMotionDatabase`, `GFBodyPerformance`; `addons/body/` motor, navigation, motion source, clip motion, clips, clip graph, executors; the gym's steering, stage and clip browser

**Answer.** Nothing needs its own slice before MM4. Two small changes are MM4's first commits, because without them MM4 would add a second model: remove the unused locomotion-node seam, and give a cut frame its own travel inside the clip composition. The brief settles four seams: one source per body, a typed intent, the source's owner contract, one database owner. One question goes to the maintainer before MM6: what moves a character with no pack.

## Disagreements

| ID | Concept | Owner | Sites | Consequence | Done when | Status |
|---|---|---|---|---|---|---|
| mm4-D1 | Locomotion enters the tree three ways: the stand-in's thresholds, the plan's cuts, and an unused locomotion-node seam | MM4 | `GFBodyClips` `install_locomotion_node`, `play_locomotion_node`; blink's `authored_closure` branch; `docs/body-api.md` | the matcher would be a fourth way in | MM4's first commit removes the seam, its two probes and its doc paragraph | new |
| mm4-D2 | A cut frame's travel: `motion_delta` is zero on a cut frame, and playback reads the root track instead | MM4 | `GFBodyClips` `motion_delta`, `travel`; `GFBodyPlayback` | a matcher cutting about six times a second loses a frame of travel per cut | MM4's second commit takes a cut frame's travel from `clips.travel` and playback drops its copy | new |
| mm4-D3 | The motion database in four shapes with no owner of its invariants | MM4 | `tdoll-motion` `Group`, `Row`; `records/motion.rs` `Header`; `GFMotionDatabase` | each shape checks a different subset, and MM4 would add a fifth | one `Database` in `tdoll-motion`, built once at native load and read by `explain` | new |
| mm4-D4 | One dictionary carries three intents: segment move, heading-only turn and steering | MM4, then MM6 | `GFBodyNavigation`; `GFBodyMotionSource`; `GFGymSteering` | the matcher would guess which intent it was given | a typed intent, `Steer` in MM4; `Path` and `Face` in MM6 | new |
| mm4-D5 | The source's lifetime is inferred: one shared source, and `rest()` means both "play idle" and "reset" | MM4 | `GFBodyMotionSource` `rest`; `GFBodyInteraction`; `GFBodyLocomotion` | a matcher loses its continuity across steer-to-move | reset is separate from `rest()`, and every change of owner aborts the source | new |
| mm4-D6 | The stand-in and the matcher side by side | MM6 | `GFBodyPerformance` `locomotion_is_stand_in`, `locomotion_time_scale`; `GFBodyClipGraph` rate | two sources to keep correct until one goes | MM6 removes the stand-in, or keeps it as the named fallback | new |
| mm4-D7 | Characters with no pack have nothing to move them once the stand-in goes | the maintainer | Paperbot, Gymbot, CesiumMan, runtime imports; the body suite's fixture | MM6 would strand every packless character | the maintainer decides before MM6 | new |

## Models in play

- Rust: `tdoll-motion` owns the feature layout (36 columns as `Group`, f64 `Row`, horizons in seconds), extraction, `Normalisation`, `Coverage`; no database type, no search. `stages/motion.rs` poses capture clips. `records/motion.rs` owns the wire `Header` and f32 `motion.bin`; `explain/database.rs` reads it back.
- Doll contract: `GFMotionDatabase` (flat f32 arrays), loaded by `GFDollAnimation._motion`, read by nothing after. `GFBodyPerformance` holds the stand-in's walk, time scale, stand-in flag, four turns and idle.
- Body: `GFBodyMotor` (sole transform writer); `GFBodyNavigation` (path, arrival, stop; builds an intent dictionary or a heading-only intent); `GFBodyMotionSource` (the source contract, dictionary intent, one per body); `GFBodyClipMotion` (the stand-in: thresholds, rate from speed, stride); `GFBodyClips` (base via crossfade or cut, locomotion-node seam, overlay, `motion_delta`, `travel`, `clip_planar_speed`); `GFBodyClipGraph`; `GFBodyGestureEnvelope`; executors `GFBodyLocomotion`, `GFBodyInteraction`, `GFBodyPlayback`.
- Gym: `GFGymSteering` (WASD to a steer action; stops the source when no key is held), `GFGymStage`, `GFObstacleDisks`, the clip browser.

## Overlaps

- Three ways to put locomotion in the tree: the stand-in's thresholds; the plan's `play_at` cuts with inertialisation; the unused `install_locomotion_node`/`play_locomotion_node` seam from the plugin trial (two probes, blink's `authored_closure` branch, `docs/body-api.md`).
- Two ways to change the base (travel crossfade or cut); a matcher stop would mix them unless the brief chooses.
- Two ways to get travel: `motion_delta` is zero on a cut frame; `clips.travel` reads the root track and playback uses it. A matcher cutting about six times a second would lose a frame of travel per cut or copy playback's workaround.
- The database in four shapes with no owner of its invariants: core f64, wire header and bin, `GFMotionDatabase`, MM4's native copy. Rows are committed twice, an accepted cost.
- One dictionary carries three intents: segment move, heading-only turn, steering with `distance: INF`.
- The source's lifetime is inferred: one shared source for move, interaction and steer; interaction alignment can leave it un-aborted; `rest()` means both "play idle" and "reset" and every action start calls it, breaking a matcher's continuity across steer-to-move.
- Two places could smooth steering: the refactor list's "trajectory spring" for steering vs MM0's spring in `tdoll-motion`.
- Names: "motion" means the database, the pack bindings, the runtime source and the Blender sampler; "clip" is three Rust types; the database names (set, clip) while the body plays "set/name"; features use the ground role's joint and travel uses each clip set's `root_motion_bone`, assumed equal and unchecked.
- Navigation grounds transforms; steering keeps height (minor).

## Half-done transitions

- Stand-in vs matcher: `locomotion_is_stand_in` read by nothing; `locomotion_time_scale`, the clip-graph rate and `PLANTED_FRAMES` wait for MM6.
- O3DE-only vs both packs: MM4 adds EMM to `makiatto.kdl`, rebuilds, commits EMM clips and the ~16 MB database twice; the earlier pack cuts are undone if MM4 shows a gap.
- Navigation gives only the next segment; the path ahead is MM6's.
- The finish pose and the source owner were meant to come before the matcher.
- Two MM0 questions are still open: a second native owner, and checks above unit tests.
- Disk derivation to a runtime owner in MM5; ARM64 build never run; `docs/motion-diagnostics.md` and `docs/doll-format.md` go stale with MM4.

## What later phases need

- MM5: footprint columns and clip boundaries from the database owner; the last match; obstacle disks as plain values a runtime owner reads.
- MM6: path and face intents, re-target keeping the trajectory, idle and stop on matched data, the stand-in gone.
- magrasea drives the body through actions, never steering, so the matched source must serve navigation's intents.
- New characters: packless characters (Paperbot, Gymbot, CesiumMan, runtime imports) have no answer; the body suite walks Paperbot on the stand-in. MM6's removal closes this off unless every walking character gets a pack (and the suite a fixture database) or the clip source stays as the named fallback.
- Headset: horizons in seconds, f32 rows, core/binding split and inertialisation as a modifier serve it; search time and ARM64 unproven.

## Recommendation

First commits of MM4: (1) remove the locomotion-node seam (`install_locomotion_node`, `play_locomotion_node`, `_forecast`, `_motion_state`, blink's branch, the doc paragraph, the two probes; ~40 lines); (2) the composition takes a cut frame's travel from `clips.travel` so `motion_delta` is right on every frame and playback drops its copy (~30 lines; the change report must move nothing for playback).

Not yet: removing the stand-in, the clip-graph rate, the unread flag, the trial check, navigation's path, f16 rows, the finish pose.

MM4's brief says where the design is going: one motion source per body chosen at binding from the skin, never swapped live; a typed intent (`Steer` now, `Path` and `Face` in MM6) reduced to one desired trajectory at the database's horizons, smoothed only by the spring in `tdoll-motion`, `steering.gd` mapping keys to a velocity, zero velocity still a steer; the owner contract (reset separate from `rest()`, abort on every change of owner); one `Database` in `tdoll-motion` (f32 rows, layout from `Group`, selectable ranges, normalisation) built once at native load from `GFMotionDatabase`, resolving (set, clip) to animation names and checking the ground joint is each set's root bone, read by `explain` too; transitions are cuts plus inertialisation, with the stop to idle chosen; names `GFBodyMatchedMotion`, `GFMotionMatcher`. The maintainer decides on a second native owner before code; MM4 adds EMM; the packless question comes before MM6, which MM4 must not depend on.
