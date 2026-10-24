#!/usr/bin/env bash
set -euo pipefail
python3 -I /tests/grade_adherence.py review \
    --review-file /app/REVIEW.md \
    --spec-file /tests/review_spec.json \
    --standards-root /tests/live_standards \
    --verifier-dir "${verifier_dir:-/logs/verifier}"
