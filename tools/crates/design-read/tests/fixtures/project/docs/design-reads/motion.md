# Motion design read

- Area: clip playback and the matcher, and the records the bake writes for them
- Last full read: 3ddf4c52ef5079f3f4f977b1e3c62721dccc8b1f
- Inspected: `src/clips/`, `src/matcher.rs`, `records/motion.rs`

## Models in play

- A clip: `src/clips/clip.rs` `Clip` holds its frames; the matcher reads the same frames through `records/motion.rs` `Frame`.

An example of a row, which the check does not read:

```markdown
| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
```

## Overlaps

- Frame rate is held by both `Clip` and `Frame`.

## Half-done transitions

- None.

## What later work needs

- Root motion read from one place.

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| Frame rate | `src/clips` | `src/clips/clip.rs` `Clip::rate`; `records/motion.rs` `Frame::rate` | The two rates drift and the matcher picks frames at the wrong time. | One rate, held by `Clip`, and `Frame` reads it. |
| Root motion \| translation | the maintainer | `src/matcher.rs` `root` | The matcher and playback move the root differently. | Both read the root from the clip. |
