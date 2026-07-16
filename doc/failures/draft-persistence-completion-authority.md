# Draft Persistence Completion Authority

## Invalidated Approach

The first Phase 3 implementation let callers pass a public draft-save outcome and a save token separately into the persistence service. It also let every older in-flight completion or reconciliation rearm the autosave timer unconditionally.

That made durable success forgeable or cross-routable without the exact executor result and allowed an older save to overwrite the deadline established by a newer committed autosave setting.

## Why It Failed

A generation tuple is not globally unique across independent draft services, and a constructible `Committed` enum is diagnostic data rather than durability proof. Only the executor that validates the exact current draft, submits the exact home command, and checks its exact healthy-generation receipt can issue a completion that may publish success.

Timer authority has the same ordering requirement. A save request captures the timer generation that existed when it started. If a newer setting publication advances that generation while the save is pending, the older completion cannot replace the newer interval or publication-time anchor.

## Course Correction

The service consumes one opaque executor-issued completion bound to the complete save attempt and rejects results that do not match its exact in-flight request. Public diagnostic status values are not accepted as commit proof.

Save completion and same-home reconciliation rearm the timer only when the request still owns the current timer generation. A newer committed setting publication always retains its interval, revision, generation, and deadline anchor.

Fault tests cover whole-old or whole-new seven-record creation, actual executor ambiguity through same-home recovery and handle reacquisition, and current-draft reads racing a durable publication.
