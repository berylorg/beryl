# Discussion Context Projection Readiness

## Status And Boundary

Accepted on 2026-10-20 after independent architecture-readiness review across the four owning
documents. This boundary specifies the existing discussion-context behavior for ordinary execution;
it does not accept source implementation, a provider/model run, complete child conversation or
discussion status/Retry presentation.

The [feature](../features/branch-discussions/design.md#first-submission) requires the exact immutable
selected assistant passage once before the first discussion-local input. The
[CAS-live system](../systems/cas-live-syndic-transcript/design.md#branch-selection-context) owns the
cross-package establishment contract. Syndic's
[history boundary](../../crates/syndic-storage/doc/design-history-storage.md) and
[V7 schema](../../crates/syndic-storage/doc/design-schema-v7.md) own authenticated storage preparation
and persisted bytes. The [App projection authority](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md)
owns orchestration, live-target custody and publication.

## Evidence And Review Scope

Pinned [CAS 0.146.0 source research](../memory/github.com/berylorg/codex-fork/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/assistant-context-injection.md)
establishes the stable assistant-message injection primitive and its flushed idle-history outcome.
The existing Beryl adapter supplies consumed idle-target custody for both newly started and forked
targets. This evidence does not establish live current-version runtime qualification or
provider-side idempotency/readback.

An independent construction of the specified fixed-width ASCII header and trailer confirms
430 header bytes, 31 trailer bytes and 461 total overhead bytes. A 65,536-byte accepted selection
therefore produces a 65,997-byte frame; transport must span bounded pages. This arithmetic check
does not establish a source encoder or runtime result.

The flat lineage encoding has a 180-byte maximum common prefix and maximum complete sizes of
397 bytes for native seeding, 310 for inherited native context and 237 for replay. These bounds
include the retained context ordering floor. The proof retains one source anchor at most; nested
discussion depth does not enlarge the encoding.

Readiness review covers immutable envelope and original owner identity, selected-prefix context
inclusion, insertion ordering, first-input replacement, nested discussions, parent divergence,
native terminal cutoffs, continuation/resume, bounded complete recovery, exact frame bytes and
accounting, ordinary-byte preservation, source/tool/target-generation fences, cancellation and
consumed-target injection/publication uncertainty.

Independent review found no unresolved blocking guarantee across ownership, bounded nested ordering,
native-cut inclusion, complete recovery accounting, source/generation fences and consumed-target
failure custody. It independently confirmed the frame and proof dimensions. Its sole consistency
correction aligned the history public-bounds summary with context-aware projected items while
preserving ordinary V1 semantics; that correction is included in acceptance. Scoped diff checking
passes. Existing lineage tags 0/1, recovery V1 bytes and all 103 storage families remain unchanged.

Implementation acceptance must independently qualify actual ordinary execution, typed backend
ordering and failure custody. The preflight, native preparation and recovery guards remain closed
until the complete context-aware source path exists; documentation acceptance alone grants no
execution capability. No model or provider runtime was run for this boundary.
