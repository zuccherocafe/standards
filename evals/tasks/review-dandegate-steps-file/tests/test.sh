#!/bin/bash
# Scores the review in a fresh copy of the task's image, without network: the findings the agent phase captured as
# /var/tmp/findings.jsonl against the known defects in expected.json, by the scorer the image holds.
exec bash /opt/findings/score.sh /var/tmp/findings.jsonl /tests/expected.json /logs/verifier/reward.json
