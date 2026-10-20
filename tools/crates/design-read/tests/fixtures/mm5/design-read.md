# MM5 design read

- Area: the matcher's footprint and obstacle columns, and where obstacle disks are derived
- Revision: 4f1c0d2a9b8e7f6a5d4c3b2a1908f7e6d5c4b3a2
- Previous: ../mm4/design-read.md
- Inspected: `tdoll-motion` `Group`, `Database`; `GFBodyMatchedMotion`; `GFObstacleDisks`; `GFBodyNavigation`

## After the last phase

MM4 landed its two first commits, so the locomotion-node seam is gone and a cut frame carries its own travel. The database has one owner in `tdoll-motion`, and the source's owner contract separates reset from `rest()`. The typed intent has only `Steer`; `Path` and `Face` remain MM6's. The stand-in and the packless question are outside this read's area and stay where MM4 left them.

**Answer.** MM5 can add footprint columns to the one database without a slice of its own. One new disagreement: obstacle disks are derived in the gym and again in navigation.

## Disagreements

| ID | Concept | Owner | Sites | Consequence | Done when | Status |
|---|---|---|---|---|---|---|
| mm4-D1 | Locomotion enters the tree three ways | MM4 | `GFBodyClips` | none left | the seam is removed | resolved |
| mm4-D2 | A cut frame's travel | MM4 | `GFBodyClips` `motion_delta` | none left | a cut frame's travel comes from `clips.travel` | resolved |
| mm4-D3 | The motion database's owner | MM4 | `tdoll-motion` `Database` | none left | one `Database` owns its invariants | resolved |
| mm4-D4 | One intent type with only `Steer` so far | MM6 | `GFBodyMotionSource` `Intent` | navigation still reduces a path to a steer | `Path` and `Face` exist and navigation sends them | open |
| mm4-D5 | The source's lifetime | MM4 | `GFBodyMotionSource` `reset` | none left | reset is separate from `rest()` | resolved |
| mm4-D6 | The stand-in and the matcher side by side | MM6 | `GFBodyPerformance` `locomotion_is_stand_in` | two sources to keep correct | tracked in MM6's brief | parked |
| mm4-D7 | Characters with no pack | the maintainer | Paperbot, Gymbot, CesiumMan | MM6 would strand them | tracked as the maintainer's decision before MM6 | parked |
| mm5-D1 | Obstacle disks are derived twice | MM5 | `GFObstacleDisks`; `GFBodyNavigation` | the matcher and navigation would avoid different obstacles | one runtime owner derives the disks and both read it | new |

## What later phases need

- MM6: the path intent closes mm4-D4 and needs the disks' one owner from mm5-D1.
