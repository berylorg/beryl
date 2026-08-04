# Scope

Phase 13 revision-bound recovery cursor replay across Syndic, the app source broker, and backend
`thread/inject_items` encoding.

# Invalidated Approach

The app mapped every Syndic recovery-cursor error to the backend's generic `ReadFailed` source
outcome. The request still failed closed, consumed and abandoned the fresh target, and preserved
completion-unknown after possible dispatch.

# Evidence

The bounded-resource system contract requires stale revision, cancellation, decode rejection, and
dependency failure to remain distinct typed causes. Syndic already emitted
`RecoveryProjectionError::ConcurrentChange` when the cursor's exact source revision drifted, but the
app erased that cause before crossing the capacity-one broker. The focused mid-replay drift test had
therefore proven safe abandonment while asserting the wrong generic taxonomy.

# Why It Failed

Completion certainty and failure cause are separate facts. Several source failures correctly share
completion-unknown and target abandonment after dispatch, but collapsing their causes prevents
callers and diagnostics from distinguishing an obsolete revision capability from a durable read
failure or an invalid source/proof. The generic mapping contradicted system authority even though it
did not permit unsafe retry or false success.

# Course Correction

`ThreadInjectionSourceError` now keeps cancellation, broker unavailability, revision drift,
dependency read failure, and invalid durable source distinct from each other and from structural
page disagreement. The app maps `RecoveryProjectionError::ConcurrentChange` only to revision drift,
maps an underlying durable read error only to `ReadFailed`, and exhaustively maps the remaining
proof/content rejections to `InvalidSource`. Backend serialization preserves the exact cause while
dispatch progress independently decides connection invalidation; every failure still consumes and
abandons the fresh target.
