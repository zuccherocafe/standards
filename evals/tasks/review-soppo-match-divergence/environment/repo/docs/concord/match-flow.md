# Match-flow design read

`TypedStmtKind::diverges` recognises direct returns. Both missing-return checking and nil narrowing consume that fact. Match statements conservatively fall through, even when every variant has a returning arm. This refuses some valid programs but never infers termination merely from a guarded arm's presence.

The planned change adds enum coverage to that shared predicate. `MatchArm::guard` is retained in the AST, and a false guard allows continuation when no later arm matches. `GateVariant` is a closed set with `Open` and `Closed`. The CLI owns source-file reads; the core computes over values.
