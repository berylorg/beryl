# Frozen Thread Catalog Readiness

## Boundary And Evidence

This is source-backed readiness evidence for the exhaustive immutable Thread Switcher collection,
observed on 2026-10-08 at Beryl `50f6e507`. It does not qualify production frozen queries or the
visible flyout. Authority remains the conversation-threads feature, Beryl-home storage system and
the HomeStore, State and app package designs and their catalog supplements.

## Existing Capabilities

- HomeStore bounded point/cursor reads validate exact domain registration and typed envelopes on
  one per-call coherent snapshot. Generation admission and attachment retirement already close
  new access and drain admitted users. `read.rs` does not retain a snapshot across calls.
- State `catalog/acquisition.rs::CatalogCurrentScan` checks a Catalog revision before and after
  bounded recency pages and validates primary/index agreement. It detects drift; it cannot preserve
  historical rows while later commits proceed. The Catalog domain has primary and recency families
  and no retained query registry. Running inventory is a process-work subset, not an exhaustive
  durable collection.
- `catalog/value.rs::CatalogSearchFields::matches` provides the admitted canonical NFKC-casefold
  substring semantics. Recency keys use descending activity and ascending stable thread identity.
  Exact source revisions identify Syndic summary, runtime, root and optional claim records.
- `catalog_projection.rs::prepare_thread_catalog_projection` compares complete validated source
  facts and revisions, then prepares one atomic cross-domain repair or exact-current result for a
  named thread. Its production caller is acquisition repair in `window_acquisition.rs`; no complete
  ordinary catalog maintenance service is mounted.
- The core `main_window/running_threads/activation.rs` prepares exact idle-thread claim replacement
  without Running membership. The shell entrance adds Running membership and open-elsewhere reveal
  policy and cannot be the Thread Switcher entrance. Ordinary failed-home selection custody still
  needs separate qualification; creation recovery is not its substitute.

## Source Currentness Counterexamples

`CatalogFreshness::Current` alone is not a source agreement proof in the present production graph.

- `running_owner/ordinary_close_session.rs::execute_with_admission_hook` composes only Session
  removal. `session/removal/mutation.rs::RemoveCapturedWindow::contribute` deletes both claim
  indexes without a Catalog update or stale marker. The existing Current row can keep the old claim.
- `cas_projection/publication.rs::admit_live_event` dispatches a Syndic current-domain command.
  `syndic-storage/src/mutation/live/event.rs::LiveSourceEventMutation::records` advances canonical
  history activity and completeness; `EventRecords::contribute` publishes HistorySummaries without
  compact ThreadCatalogSummaries or State Catalog publication. HomeStore
  `writer.rs::execute_current_admitted` constructs exactly one mutation participant.
- Production-source search found no caller of `CatalogState::mark_stale`; the observed invocation
  is a State test. Creation and claim replacement explicitly compose catalog effects, but that does
  not cover the demonstrated source producers.

Checking only the stored Syndic summary revision does not close the live-history gap: canonical
history can advance while the compact summary revision stays unchanged. Syndic
`read/catalog_summary.rs::prepare_thread_catalog_summary` authenticates its canonical sources and
may derive a replacement. Its history-title fallback must not become implicit flyout history loading.

These are a distinct producer/currentness prerequisite to the read capability. They do not justify
omitting stale rows, publishing mixed revisions or treating Running membership as the catalog.

## Actionable Prerequisite Decomposition

The first production prerequisite is a distinct typed current cross-domain command plus bounded
writer-time named-thread Catalog invalidation. Existing HomeCommand composition is reusable, but
CurrentDomainCommand explicitly prohibits multiple domains. Preserve that contract, use the
ordinary writer/preparation/batch/outcome machinery, and add no retry or sidecar route. Invalidation
must authenticate both row copies inside the writer and reject missing or contradictory records.

The next prerequisite joins those effects into the finite existing producer families: draft
checkpoint publication; input acceptance/promotion; live event and item/transcript convergence;
compaction continuation and incomplete recovery; discussion archival; claim restoration, activation,
ordinary removal and interrupted-close recovery. Creation, same-window acquisition, ordinary claim
replacement and abandonment already join Catalog effects and require regression qualification.
Representative source anchors are `draft_piece/publication.rs::PublicationMutation::contribute`,
`mutation/admission/shared.rs`, `mutation/promotion/records.rs`, `mutation/compaction/continuation.rs`,
`mutation/repair/incomplete.rs` and `mutation/discussion_handoff.rs` in Syndic; app composition is in
`composer_host/publication.rs`, `input_admission.rs`, `cas_projection/ordinary/converge/`,
`discussion_settlement/admission.rs` and ordinary Session/close/recovery owners.

That producer boundary also establishes exhaustive compact-source completeness and coherent
rebuild readiness before any query is admitted. A Catalog-only scan cannot discover a Syndic thread
whose row is missing. The existing compact-summary fallback may inspect history; maintaining
source-owned compact summaries before the reader is admitted is required, rather than hiding
history scans behind opening/filtering a flyout.

Generated-title acceptance and runtime/root availability setters have typed APIs but no observed
production app callers. They remain explicit future producer admission gates. Scope-wide changes
cannot update an arbitrary number of rows in one bounded command: compact atomic scope invalidation
and bounded rebuild custody must be qualified before mounting such a producer. Presentation-only
root activity and source-witness revision policy must be distinguished. This readiness acceptance
does not choose a new durable fanout representation or certify those unmounted setters.

This is finite technical prerequisite work; no product-level choice or CAS replacement was found.
The primitive boundary is actionable now. Producer readiness and frozen production queries remain
unaccepted until their separate qualification boundaries pass.

## Required Frozen Capability

HomeStore owns the retained snapshot inside its generation; external identities are non-owning.
State owns complete criteria, canonical filtering, exact count, deterministic order, finite page
and exact position semantics. The published app graph owns query custody and worker requests and
exposes only a non-owning reader. Every terminal path releases the exact retained read; retirement
drains admitted work and drops every snapshot before database disposal.

The dependency ownership proof is retained in
[generation-owned frozen reads](../memory/github.com/berylorg/fjall-fork/commit/9c1ed4537186ea1eac6d57e8affcc31ebd367546/generation-owned-frozen-reads.md).
An escaped Fjall Snapshot owns a database clone and version pin, so it cannot be returned as a
caller-owned replacement for this boundary.

Complete compact evaluation/counting may be proportional to the durable catalog under the existing
system envelope. Its state remains bounded. The first coherent count and first presentation page
do not require constructing all presentation rows. No unknown-count widget mode or substring-index
redesign follows from this readiness finding. Pages must accommodate valid 256-KiB catalog records
and their encoded overhead rather than copying an unrelated smaller widget ceiling.

## Qualification Required Before Mounting

Producer acceptance must prove atomic update-or-stale coverage of catalog-affecting source commits,
bounded rebuild custody and coherent readiness without flyout-triggered transcript loading. Frozen
query acceptance must prove exhaustive empty-draft inclusion, all-scope and normalized search,
equal-recency ties, count/page/position agreement across concurrent writes, largest valid rows,
foreign/released/exhausted identities, cancellation, stale completions, release and actual Home
retirement with outstanding tokens. Ordinary activation failed-home custody remains separate.

No production source or Cargo manifest changes belong to this readiness boundary. Its completion
requires independent review of the authority and exact prerequisite decomposition.

## Readiness Acceptance

Independent review on 2026-10-08 cleared the authority and source-backed decomposition for the
atomic current cross-domain and bounded Catalog invalidation prerequisite only. It corroborated
the one-participant current writer and producer gaps, and found no product-level choice. Source
completeness, compact-summary maintenance, scope fanout and full producer readiness remain explicit
gates before production frozen query implementation. This is not GUI or live-CAS qualification.

Documentation checks passed; semantic reconciliation was Current with zero failed/blocked chunks.
