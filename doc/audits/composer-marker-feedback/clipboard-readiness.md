# Mounted Clipboard Admission Readiness

Source assessment at Beryl `4d06a442783ed213f686f589eed98b2b7091bed9`, 2026-10-04.
Disposition: **not ready for production clipboard implementation**. This report is diagnostic
evidence, not design authority or acceptance of cut, paste or the complete rework item. No source,
dependency, test or native GUI change is part of this investigation.

## Controlling Requirements

The [composer](../../features/composer/design.md#shared-text-input-integration) requires a captured
insertion/replacement range, visible paste-pending gating, pre-admission Escape cancellation and one
atomic undoable adoption. Ordinary refusal restores writable unchanged state; ambiguity retains exact
intent/evidence and suppresses dependent edits. The [clipboard semantics](../../features/composer/design.md#image-clipboard-semantics)
require complete successful clipboard writing before cut, stable same-conversation identity while
eligible, foreign-conversation label allocation, and usable cut undo after a later refused paste.

The [image feature](../../features/image-assets/design.md#paste-and-draft-outcome) owns visible image
paste outcomes. The [image system](../../systems/image-assets/design.md#bounds-and-recovery) requires
reservation before retained demand and release on every terminal/disposal path; clipboard tokens
are non-durable. The [app adaptation](../../../crates/beryl-app/doc/design-catalog-and-composer.md#edit-marker-and-candidate-adaptation)
owns exact evidence/replay and opaque settlement transport. The [composer GUI](../../features/composer/gui.md)
owns propagated clipboard commands and contributes mutation feedback to the existing Notifications
arbiter. No additional panel or slot follows from this work.

Composer, image feature, image system and app select `production-application/v2`; applicable feature
and app external-effect guarantees protect clipboard acknowledgement and exact draft adoption.
The image feature also selects `irreversible-operation/v2`. This evidence phase requires semantic
completion review; later consequential production changes require independent semantic review.

## Actual Consumers And Capabilities

- [`construction.rs`](../../../crates/beryl-app/src/main_window/conversation_composer_owner/construction.rs)
  checks `mutation_gated` and emits `RichPastePropagated` with the selected host identity. That
  identity does not capture widget caret/directed selection or create a pending paste.
  [`conversation_composer_mount.rs`](../../../crates/beryl-app/src/main_window/conversation_composer_mount.rs)
  rejects stale owner identity and re-emits paste and clipboard-limit events. Exact-identifier
  searches across app Rust source and shell find no downstream production consumers for either.
  The mount witness in `tests/main_window_composer_mount.rs` observes propagation only.
- [`clipboard/collection.rs`](../../../crates/beryl-app/src/main_window/conversation_composer_owner/clipboard/collection.rs)
  accepts only `ClipboardProvenancePolicy::Omit`; receiving provenance cancels and errors. Its
  coordinator already owns bounded collection, successful-write acknowledgement and exact cut
  deletion. The [widget API assessment](../../memory/github.com/berylorg/gpui-text-input/commit/5eb31033c95ae2ce3187363ab127d218d7a5413b/hosted-rich-clipboard-boundaries.md)
  identifies the usable streamed provenance and host mutation APIs. There is no app private-token
  producer, staged provenance consumer or eligible rich-paste source in this integration.
- [`production_clipboard_writer`](../../../crates/beryl-app/src/main_window/conversation_composer_owner.rs)
  writes fallback text with `ClipboardItem::new_string` and always returns `Written`. The
  [pinned Windows GPUI assessment](../../memory/github.com/berylorg/zed-fork/commit/1664626feb9e4904b7d8bf4016cbed42da558536/clipboard-admission-boundaries.md)
  proves the public write API has no acknowledgement and reads allocate complete representations
  before returning. Metadata constructors exist, but native text-hash matching does not establish
  private Beryl eligibility. An injected successful writer is not native cut acceptance evidence.
- [`ComposerHostImageMarkerMetadata::from_source`](../../../crates/beryl-app/src/composer_host/mutation.rs)
  transports typed candidate/cut/accepted selectors into existing evidence readiness. Syndic's
  [`Cut` resolver](../../../crates/syndic-storage/src/draft_piece/admission/readiness_source/proof.rs)
  authenticates an exact committed settlement, successor generation/root, predecessor occurrence
  and absence of that marker in the successor. It requires the origin session's newest candidate
  to remain that exact successor. Undo, redo or another adopted edit can invalidate this selector.
  Candidate/cut proof accepts only the destination conversation with preserved label; accepted
  sources have a separate foreign allocation path. This is not a generic foreign cut-token API.
- [`HomeStore::admit_sidecar`](../../../crates/beryl-home-store/src/sidecar/operations.rs) accepts
  a complete borrowed byte slice and explicit length limit, returning generation-qualified
  `AdmittedSidecar`. [`AssetState::publish_metadata`](../../../crates/beryl-state/src/asset.rs)
  consumes admitted evidence; fresh-asset readiness then authenticates ordinary committed metadata.
  These are usable durable primitives, but they do not supply bounded clipboard acquisition,
  paste queue reservation, captured editor intent or cancellation ownership. Exact search finds
  `paste_admission_queue_items` in system authority, with no corresponding app/state Rust consumer.

## Material Readiness Gaps

1. **Native clipboard boundary:** copy/cut needs truthful complete-write acknowledgement, including
   metadata failure, and paste needs caller-bounded representation inspection/acquisition before
   whole-value allocation. Current public GPUI calls cannot supply either guarantee. The owning
   GPUI package boundary and Beryl consumer assumptions need reconciliation before implementation.
2. **Private representation custody and eligibility:** the image system says tokens are non-durable
   but does not choose their live owner, bounded provenance backing, retention budget, replacement
   or expiry, home/session invalidation, or promotion from successful copy to exact committed-cut
   provenance. Define these in image-system and package authority, with feature-visible expiry
   outcomes in composer authority. A capped text string cannot bound arbitrarily many zero-width
   provenance entries; the streamed protocol must not become a final resident marker collection.
3. **Reuse after cut and foreign paste:** define whether tokens intentionally expire on every later
   origin candidate transition or retain a separately authenticated source. Define the source
   treatment for a draft-only marker pasted into another conversation; existing candidate/cut
   selectors cannot allocate foreign labels. Substituting `FreshAsset` would be a material provenance
   choice, not an app-private adaptation. Readiness must cover copy without deletion, failed cut,
   committed cut, later refused paste, undo/redo, history eviction and session replacement.
4. **Captured paste lifecycle:** choose the app/system owner of an immutable bounded clipboard
   source, exact caret/directed range and endpoint proofs, pending gate, Escape cancellation and
   source replay through evidence/staging. Define retirement and home replacement custody, and
   how cancellation changes at durable admission. Existing mutation protocols supply settlement
   primitives; a propagated event alone supplies no lifecycle. General rich replacement must use
   one host operation, not repeated direct-marker insertion.

## Recommended Resolution And Acceptance

Reconcile the composer, image-system, app and owned GPUI package authorities as one bounded
clipboard boundary before scheduling production mounting. Keep existing Syndic label admission,
fixed profile, evidence/replay and opaque outcome flights. Resolve the material token/source
choices above explicitly, then derive implementation and qualification phases from those decisions.
Do not compensate with readback heuristics, whole-value reads followed by a size check, a resident
marker vector, fabricated labels or automatic reversal of a cut.

Later qualification must exercise actual mounted copy/cut/paste, truthful native failure seams,
oversized and malformed text/metadata/image rejection before allocation, exact selection capture,
stale source/owner completion, cancellation on both sides of admission, typed size/capacity/storage
refusals, ambiguity custody, one-step undo/redo and bounded release after repetition. A successful
cut followed by refused paste must preserve eligible clipboard custody and usable cut undo.
The [accepted large-draft direct-marker evidence](qualification.md) remains reusable within its
documented scope; it does not prove these clipboard outcomes.

Verification for this report is exact source/API and authority inspection, identifier coverage,
pin agreement and semantic completion review. No Cargo test or native clipboard operation was run.
Independent semantic review accepted the investigation evidence with no blocking inaccuracies;
the public metadata method name was corrected to `AssetState::publish_metadata`. Six changed
Markdown files passed local-link and header-spacing checks. Production readiness remains blocked.

## Subsequent Authority Resolution

Operator approved the recommended clipboard decisions on 2026-10-04. The owning composer,
image-system, app, Syndic and GPUI authorities now define acknowledged native writes and reads
bounded before allocation; one process-owned compact source with exact live-candidate/cut-successor
eligibility and expiry; authenticated foreign candidate/cut label allocation; and one captured
paste owner using ordinary evidence/replay and exact settlement. Home-store authority defines the
bounded streaming sidecar prerequisite. The original source assessment remains historical evidence:
those APIs are implementation work, not capabilities already present at its baseline.

The source descriptor replays from existing immutable paged roots and retains only a compact
provenance closure, avoiding a new clipboard database or marker-sized resident collection. A cut
temporarily makes its token unavailable and promotes it only after exact adopted success. Refused
destination paste retains eligibility, while later origin adoption or retirement expires it.
Native read/write, eligible copy/cut, mounted paste and full large-draft qualification remain
separate acceptance boundaries in the root plan.

Independent architecture review accepted the resolved boundary after adding an exact origin fence
inside Syndic's sole destination mutation participant. The command validates the still-active latest
origin atomically with destination admission; later expiry cannot revoke admitted replay, and
already-admitted reconciliation uses ordinary immutable settlement. The fence changes no durable
format. The malformed/stale-metadata versus absent-metadata distinction is explicit. No material
architecture choice remains for the planned slice; production capabilities and workflow acceptance
remain pending. The GPUI fork's existing `production-application/v1` review requirement was retained.
