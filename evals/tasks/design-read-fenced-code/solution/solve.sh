#!/bin/bash
# The landed correction, applied to the starting tree.
set -euo pipefail
git -C /app apply --whitespace=nowarn /solution/solution.patch
