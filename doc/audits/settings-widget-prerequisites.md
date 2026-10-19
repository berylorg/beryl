# Settings Widget Prerequisites

## Scope

Qualify the missing owning-widget contracts identified by the
[source-readiness correction](../failures/settings-widget-contract-readiness.md) before the complete
Settings App mount. Operator authorized direct fixes in the owned dependencies. Source and tests
must demonstrate admission before mutation rather than observer rollback.

Owning tests used nextest with eight test threads, a 32 MiB harness thread stack and bounded slow
test termination. Production checks used normal stack settings and no test-only features. Shared
build artifacts were reused; task-owned source snapshots and logs were released after review.

## Owned-Value Input

Accepted and published `gpui-text-input` revision
`b1e5c9681d3a1af1e4723e750508b2dda0c03f87` adds optional fallible normalized UTF-8 byte limits,
bounded payload-free refusal events and synchronous read-only updates on retained inputs.
Complete successors are checked before normalization allocation and atom ingestion; refused edits
preserve text, atoms, caret, directed selection, composition, scroll and history. Undo/Redo check
before removing snapshots. An exact-cap Enter reports one refusal without ancestor propagation.

Owning nextest qualification passed 60 cases across `value_limits`, `text_model` and `widget`
with `test-support`. Coverage includes normalization/multibyte boundaries, zero/unlimited and
fallible tightening, repeated rejection, atoms, programmatic reset and host drop insertion,
clipboard/typing, IME, retained native callbacks, dynamic read-only and history selection.
Normal no-feature/default-stack production check passed. Independent semantic review accepted the
source and receipts and verified the frozen snapshot matched the live changed paths. Test support
uses public editor facts and behavioral Undo/Redo traces, with no new source test includes.

Mounted tests establish production-route admission and retained-state behavior; they do not prove
native OS paint, clipboard integration outside the simulated platform or native drag/drop delivery.

## Settings Widget

Accepted and published `gpui-settings-window` revision
`093c67a2b6813a0810a9b4e7493b474072623a35` pins that input revision. It supplies independent
primary/detail field availability and validity, footer command availability/reasons, and a live
window-wide reconciliation gate. Current admission is checked before edits, row/page actions,
navigation, popup actions, footer requests and native/programmatic hide or close. Gated hide returns
an observable refusal. Field byte limits are validated before model publication and configured
without rebuilding retained editors; picker canonical values are checked before publication.

Disabled row/page/footer actions retain focus identities and expose their reason on hover or focus.
The action focus map reconciles to current selected-model actions without accumulating visited
pages; no new absolute action-count bound is claimed. Custom body renderers receive weak live
panel-lifetime admission tokens. Hosts honor these in retained callbacks and publish custom editor
read-only state with their coherent gated model; opaque custom content is not blanket-disabled.

Owning nextest qualification passed all 118 cases in `color_picker`, `field_admission`,
`interaction`, `paged_split_focus`, `paged_split_lifecycle`, `paged_split_pointer`,
`paged_split_protocol`, `panel_window` and `presentation`. Coverage includes actual mounted field
and footer gating, repeated native-close/hide refusal, stale callbacks, disabled action focus,
UTF-8/IME caps, coherent model cap changes, picker preflight, real weak panel release and bounded
paged-source lifecycle/focus/pointer behavior. Normal no-feature/default-stack production check and
locked metadata validation passed. Independent semantic/lifetime review accepted the complete
boundary and verified all live source/tests and Cargo files matched the tested snapshot.

## Beryl Consumer

Beryl manifest and lock now select exactly those two published revisions, with no other lockfile
delta. Locked no-dependency metadata validation and the affected normal production check for
`beryl-app` and `beryl` passed against canonical Git dependencies. The analyzer restarted successfully
after validation. Combined prerequisite evidence is 178 passing owning cases plus affected
production checks; independent review accepted both owning boundaries.

This prerequisite audit does not establish the App Settings service, toolbar/window mount, scalar
publication or Home reconciliation integration. Those remain the next complete acceptance boundary.
