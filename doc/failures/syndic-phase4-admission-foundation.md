# Syndic Phase 4 Admission Foundation

## Invalidated Approach

Checkpoint 3 Phase 4 was expected to add submission, accepted-input, and replacement mutations directly over the completed Phase 2 V1 schema.

That schema cannot implement the planned mutations without losing authority or performing unbounded writes:

- `DraftRecord` is the only record that retains an exact ordered `ComposerPayload`, while `CanonicalItemRecord` retains only text and cannot preserve image-marker atoms after the consumed draft is removed.
- Beryl asset references identify an accepted input or submitted item as one owner, so one input item cannot durably correlate multiple marker occurrences or assets. Draft marker labels also have no durable typed payload field.
- transcript entries are keyed only by thread and position but must all equal the one head revision, so an append or replacement path requires an unbounded atomic rewrite instead of a staged crash-safe generation.
- replacement validation requires the draft parent to become the edited turn's parent even though draft parentage is immutable.
- current binding validation conflates selected-path content with draft-only thread revision changes, so rotating a draft during active-turn admission would invalidate an otherwise exact CAS lineage.

## Why It Failed

The Phase 2 completion proof validated the declared record relationships in isolation before any production tail-advancing or draft-consuming mutation existed. It did not prove that the schema could preserve submitted composer atoms, rotate current drafts without invalidating unchanged path lineage, or rebuild a changed transcript path through bounded restart-safe commits.

The image-owner shapes also modeled lifecycle labels such as queued input as separate owners even though one accepted-input identity must survive disposition changes.

## Course Correction

Do not retain consumed drafts, serialize marker-bearing input into text, copy a valid CAS binding onto an undelivered turn, mutate immutable draft parentage, or rewrite an entire transcript index in one writer command.

Before Phase 4 mutations proceed:

- make submitted user input a typed canonical payload that can preserve every admitted atom and immutable resolved image fact;
- make durable asset ownership cardinality and identity agree with multiple markers and identity-preserving accepted-input disposition changes, or explicitly keep image admission unavailable until the later image checkpoint;
- generation-key transcript entries so a changed selected path can be assembled and published through bounded crash-safe work;
- preserve draft parentage during replacement edit and derive replacement turn parentage from the immutable target turn;
- distinguish selected-path content validity from thread revisions caused only by current-draft rotation.

## Resolution

The Operator accepted bringing the final durable image metadata boundary into Phase 4: stable marker labels, resolved submitted atoms, per-marker asset ownership, and atomic reference movement are part of the admission foundation. Image-byte admission, GUI paste, preview, and Host/WSL runtime projection remain in their later checkpoint.

The owning feature, system, and package docs now define that split. Phase 4 may resume by correcting the V1 schema directly; no compatibility reader or transitional text-only admission is authorized.

## Mutation-Gate Follow-Up

The later record-transition proof found three further omissions before accepted-input mutations could be implemented:

- a steering disposition named only a Syndic turn and could not prove the exact active binding, execution snapshot, CAS thread, and known CAS turn required by delivery;
- steering and next-turn indexes retained terminal history, so they could not also serve as a bounded live-queue authority;
- compaction and stop had no durable revisioned input-gate state against which admission could atomically choose steering, next-turn queueing, or rejection.

Do not infer those facts from process-local state, count unbounded historical accepted-input records during writer admission, or manufacture a steering target when the CAS turn id is unknown. Phase 4 mutation work remains gated until the durable accepted-input target proof, bounded live-route authority, and revisioned input-gate contract are accepted and documented.

## Mutation-Gate Resolution

The accepted correction keeps permanent accepted-input order separate from live delivery work. Every thread has one independently revisioned input gate with idle, pending-turn, steering, compaction, and stopping states, an accepted-order high-water mark, and exact live route counters. Steering retains the exact binding revision, execution snapshot, Syndic turn, CAS thread, and known-or-explicitly-unknown CAS turn; absence is never guessed.

Only nonterminal accepted inputs occupy live steering or next-turn indexes. Terminal inputs remain in permanent accepted history without consuming live capacity. Execution snapshots contain no accepted-input vector. The V1 live-work safety bound is 256 fragments and 268,435,456 logical UTF-8 bytes per thread, while total accepted history and thread turns have no corresponding count ceiling.

## Canonical-Content Follow-Up

The storage-bound audit distinguished a Fjall record from a Syndic turn and exposed another invalid assumption. Transcript projections are chunked, but `CanonicalItemRecord` still embeds assistant text in one value capped at 262,144 UTF-8 bytes. A larger CAS assistant item could therefore not remain exact canonical history even though projections and resources can represent larger rendered output.

Do not truncate the item, split one external CAS item into unrelated canonical identities, or promote rebuildable projection chunks to canonical source authority. Before live event capture proceeds, canonical item metadata must be separated from bounded ordered canonical content chunks or another equally exact range-readable canonical-content boundary. The existing 393,216-byte large-record ceiling may remain a per-record allocation policy, but it cannot be a whole-assistant-item limit.

## Whole-Draft Follow-Up

The same audit initially described `LARGE_MAX` as an acceptable envelope for a whole `DraftRecord`. The Operator rejected that product assumption: a user may paste documents sized for million-token or larger context windows, and durable composer storage must preserve them even when a selected CAS model later cannot accept the complete input.

Raising the Fjall value ceiling would only move the failure and would keep autosave, submission, canonical capture, reads, and crash recovery dependent on one unbounded value. The accepted correction is one shared chunked-content authority: small owner records reference exact manifests; bounded ordered chunks carry content; staged chunk commits remain unreachable until one atomic sealed-manifest publication; submitted and canonical owners reuse content references instead of copying whole text; and derived transcript projections never replace canonical source. Physical records and individual commands stay bounded, while one logical draft or canonical item has no fixed whole-content byte ceiling.

## Draft-Parent Supersession

Phase 55 later proved that preserving generic draft parentage was itself invalid once accepted
next-turn input could advance the selected tail without consuming the current composer. The
Operator clarified that a draft is only unsent composer state.

The current correction therefore removes `DraftRecord.parent`. Ordinary submission derives parentage
from the transaction-current thread tail; branch context and replacement use their separate typed
provenance. The earlier instruction in this note to preserve draft parentage records the
intermediate Phase 4 conclusion and is no longer target authority.
