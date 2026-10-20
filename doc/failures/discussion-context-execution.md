# Discussion Context Execution

## Context Presence Is A Selected-History Fact

Treating a missing target-owned envelope as context-free history drops inherited context from an
independent ordinary fork. `CreateThread::from_tail` preserves the selected turn prefix while
creating an independent thread-lineage root without a context owner. The typed App case
`native_discussion_seeds_one_frame_then_executes_unchanged_user_input` executed the real discussion
input, then failed with `ordinary descendant lost inherited context` in run
`61b52418-9918-4fee-9882-54d57f4f7b75`; seven other cases passed. This is failed development
evidence, not accepted execution qualification.

A related proposed shortcut inferred empty context from absence of the represented tail's dispatch
snapshot. Independent source review invalidated that inference: `live/event.rs` permits source-less
non-complete terminalization under Unbound/Stale while retaining genuine Unattempted provenance,
and `graph/dispatch.rs` admits terminal Unattempted state. The common accepted-delivery loss path
retains an activated anchor, but does not exhaust storage's supported states.

The [owning contract](../../crates/syndic-storage/doc/design-history-storage.md#exact-source-planning-without-input-admission)
now derives context from selected immutable turn-origin crossings, including independent forks.
Activated/cancelled tails can use their authenticated original snapshot and binding closure.
An ambiguous unattempted terminal cut instead requires package-issued context classification,
including an authenticated empty result; missing evidence cannot authorize a raw legacy proof.
Preparation derives the classification outside the writer, which revalidates compact source and
generation anchors. Independent correction-readiness review accepted this contract. Ordinary
persisted bytes and storage families remain unchanged. Source corrections and scoped execution
acceptance are recorded in the [qualification audit](../audits/discussion-context-execution-qualification.md).

## Numeric Generations Do Not Identify An Opening

The expanded context storage target passed twelve cases and failed four. One failure,
`reopen::context_proof_reopens_exactly_and_old_preparation_loses_generation_authority`, showed an
old opaque preparation remained usable after closing and reopening the same Home. Comparing its
durable Home id and numeric generation was insufficient: a new store opening restarts that counter.
The other three failures occurred during fixture transcript construction and are separate.

Use HomeStore's existing opaque `HomeGenerationIdentity`, which combines the private store instance
and admitted generation, for process-local preparation/classification admission. Initial and
recovery candidates expose the same checked identity without publishing themselves. Do not add
another nonce, registry or persisted field, and do not change persisted lineage encodings to fix
a process-local capability fence. The owning history and Home lifecycle contracts now state this
requirement; runtime qualification is recorded in the [acceptance audit](../audits/discussion-context-execution-qualification.md).

## Owning Fixtures Must Exercise The Real Boundary

Native-only topology fixtures do not automatically contain complete replayable canonical items or
published transcript projections. Context replay and selected-passage tests must create those facts
through the owning item/transcript mutations before testing descendants. A maximum selection also
needs one finalized projection resource: a long plain paragraph may become several resources,
whereas a single fenced code resource supplies the intended 65,536-byte selection.

The fake CAS server must emit the pinned adapter's valid bounded wire envelopes, including the
established response discriminant ordering. Fixture-only schema failures do not justify broadening
the production decoder. Likewise, provisional validation tests must explicitly request schema
validation; ordinary registration intentionally does not perform an exhaustive domain scan.

Continuation fixtures must retain loaded-target custody until the next projection owns its lease.
Dropping the sole loaded projection requests connection retirement under the existing lifecycle;
that does not test ordinary same-session continuation. Assertions about a completed source should
read its exact binding, rather than invoke pending-input native planning on its terminal tail.

## New Preparations Must Preserve Existing Failure Classes

The new readonly context classification can observe a domain revision conflict while another
retained execution publishes. Wrapping every classification error as a terminal native-planning
failure bypasses ordinary execution's existing concurrency retry and can abandon a valid retained
target. Map only the explicit concurrent-change variants into the existing ordinary conflict
class; cancellation, invalid source evidence and other failures retain their original distinction.

After a seed succeeds, stale retirement must also preserve the original native-cut proof when a
clock error or an ordering-floor refusal prevents constructing the context completion proof. A
nonzero imported turn count with no supporting native provenance fails the existing writer guard.
Keep the original fork proof as nonauthorizing stale provenance rather than weakening that guard
or inventing a context completion timestamp.

## Current Head And Retained Source Are Separate Anchors

A second ordinary input publishes a new Unbound binding head while the CAS reservation still
names the preceding usable historical source. Equating the reservation's latest usable revision
with the new current head rejects a valid same-discussion continuation. Follow the existing native
recovery rule: authenticate the current head against its separately prepared fence, and match the
reservation to the exact retained source binding revision. Preserve the original context proof
and ordering floor. One input per thread, even across an inherited child, does not qualify this
case; test a real second ordinary input on the same target with no context reinjection.
