# Rigid pieces on deforming clothing

Add rigid surface attachments to the character conversion workspace in `/app` so an accessory bone (such as a badge or name card) follows the clothing surface beneath it without deforming like cloth.

## Requirements

- Support `attach "<piece>" to="<surface-material>"` in character declarations, binding each piece to the closest triangle on the target material's retained cut mesh at rest (using edge-first X, normal Y, and orthogonal Z at the closest support point) and refusing invalid or conflicting declarations.
- During offline pose evaluation, place attached pieces and their descendants from the deformed support triangle after skeletal and spring evaluation so they stay at their authored placement at rest and move rigidly with the surface.
- Ensure derived caches account for attachments.
- This is an unattended run with no interactive user replies; use your engineering judgement for any unstated implementation details and implement, test, and commit your complete change on the working branch in `/app`.
