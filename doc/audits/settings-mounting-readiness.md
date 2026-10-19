# Settings Mounting Readiness

## Boundary And Authority

Source-backed readiness at `ed1cf7c9`, following the accepted composer-history mount. This record
distinguishes live prerequisites from the remaining Settings product workflow. It does not accept
a Settings mount or authorize using orphan source as an adapter.

Product and GUI authority are [Settings](../features/settings/design.md) and its
[composition](../features/settings/gui.md). Typed scalar authority belongs to
[State](../../crates/beryl-state/doc/design-theme-settings.md); app composition belongs to
[feature adapters](../../crates/beryl-app/doc/design-feature-adapters.md#settings-and-themes) and
[shell lifecycle](../../crates/beryl-app/doc/design-shell-lifecycle.md). The
[Home storage system](../systems/beryl-home-storage/design.md) and
[theme runtime](../systems/theme-runtime/design.md#settings-selection-and-theme-document-operations)
control persistence, original outcome custody and appearance publication. The registered external
`gpui-settings-window` owns the OS window, shell, routing, bounded detail rows and paged split lists.

The effective contract protects atomic supported-setting updates, explicit outcome uncertainty,
bounded input/presentation and exact home-generation retirement. It does not require a new durable
settings journal, a new family, backend configuration UI or an unbounded generic preference map.

## Live Prerequisites

- `beryl-state/src/settings.rs::SettingsState` exposes generation-qualified ordinary/candidate
  reads, revisions and typed `apply`. The closed five-key schema and `ApplySettings` enforce
  duplicate-free, nonempty, absent-or-exact expectations and one atomic update. Existing tests in
  `tests/settings.rs`, `settings_schema.rs` and `settings_candidate_read.rs` cover that owning layer;
  no new test execution is claimed by this readiness phase.
- `theme_runtime/service` prepares appearance from typed Settings and repository identity;
  `operations::consume_settings_outcome` distinguishes committed/reconciled-new authority from
  noncommit, uncertainty and collision. Appearance failure cannot reinterpret the durable scalar
  batch. The process graph prepares and retires these services through current startup/recovery.
- The external window supports four static sections within its 32-section limit, bounded detail
  rows and revision-bound paged split lists. Its preheated OS window is the product window itself.
  The original readiness review incorrectly treated documented field availability and window-wide
  reconciliation gating as implemented. Follow-up inspection of pinned revision
  `feb598064dcc6b5ea85ba7fad02173f3dd8f2109` found these production APIs missing; see the
  [source-readiness correction](../failures/settings-widget-contract-readiness.md).
- The running Exit producer already exposes a Settings-reconciliation availability gate. It is a
  primitive awaiting its owning Settings state, not evidence of a mounted Settings workflow.

## Missing Production Consumers

`beryl-app/src/lib.rs` declares the live `main_window`, `app_services` and `theme_runtime` modules,
but no `shell` module. `src/shell/settings.rs` and its descendants therefore provide no production
Settings owner. Their old full-model assembly and synchronous feature actions are reference
material, not accepted sources to reconnect behind the new shell.

`MainWindowShellRoot::render` in `main_window/shell/host/root.rs` ends its toolbar with Exit and
has no Settings control. The live graph has no Settings window controller, window-wide staged
draft, coherent prepared scalar presentation, Apply/OK command owner or scoped unresolved-row
state. Bootstrap diagnostics explicitly report Settings unavailable. Dynamic-tool parser/DTO files
outside the active module graph do not provide these consumers.

Applied scalar consumers also need explicit integration. Compaction timeout, Developer Instructions
and end-turn sound have live State readers in `cas_projection/context_compaction/timeout.rs`,
`cas_projection/ordinary/model/policy.rs` and `parent_completion_sound.rs`. Autosave exposes
host/mount interval-publication primitives, but no live `DraftAutosaveInterval` Settings read or
Settings-owner publication path. A successful durable Apply alone does not qualify that consumer.
Committed accepted intervals must reach current, newly opened and recovered composers; staged
drafts must not change any live scalar consumer.

The missing boundary must include the actual toolbar-to-window route, finite prepared pages,
feature validation/defaults, exact all-or-none Apply/OK, Reset/Cancel/close, original reconciliation
custody and the process Exit gate. It must also join active-theme selection to existing prepared
appearance publication, keep repository/document commands separate from Apply/OK, and release or
retain exact work through same-home recovery and final shutdown. A toolbar button alone, or a
callable settings mutation helper alone, would not complete this product boundary.

## Next Acceptance Boundary

Complete the app-owned Settings workflow under the existing feature contract. Before code planning,
make its process/service ownership, immutable source/draft identities, bounded input and command
custody, scoped unresolved keys, and fresh-generation retirement explicit in the app's owning
design. These are implementation-architecture details; no unresolved product choice has been
identified. Do not change backend configuration policy or weaken theme/replacement lifecycle.

The complete mount should qualify real toolbar entry/show/hide/reveal, static section/page routing,
largest valid and invalid-within-cap Developer Instructions drafts, atomic oversize refusal,
all-or-none multi-setting validation and persistence, defaults/rejected saved values, row Reset,
clean and modified OK, exact Apply uncertainty/noncommit/collision, original Exit gating, reopening,
active-theme outcome publication, bounded growing theme sources and stale/released request rejection.
Verify ordinary scalar consumers and committed autosave publication across existing/new/recovered
composers, including unchanged live behavior for invalid, unapplied and refused drafts.
Preserve startup, recovery, theme and ordinary shell regressions. Use owning integration targets,
production checks and complete independent semantic/lifetime review; simulated GPUI cases do not
establish native OS paint or sustained scrolling performance.

## Readiness Review

Independent source/authority review accepted this readiness boundary and the ordinary-consumer
correction, with no major architectural or technical blocker. It accepts ownership/custody design
formalization followed by the complete mount, not a Settings implementation. No source, Cargo
manifest or runtime mutation was performed for this readiness phase.

## Application Ownership Review

Independent semantic/lifetime review accepted the subsequent
[Settings application ownership](../../crates/beryl-app/doc/design-feature-adapters.md#settings-application-ownership)
and matching [theme recovery rule](../systems/theme-runtime/design.md). Original Home registry
classification controls reconciliation across service replacement. A finite applied-settings source
preserves proven values while pending and excludes only unresolved keys from new consumer work.
Same-process theme recovery retains bounded presentation context without transferring old execution
authority. No material architecture choice blocks the complete mount. This acceptance covers
authority only; implementation and runtime qualification remain outstanding.

## Source Readiness Correction

The authority review remains valid, but production source readiness is incomplete. The pinned
external Settings widget always enables its footer, hides on native close before host observation,
and constructs ordinary text inputs with only undo retention limits. It cannot enforce the
required reconciliation freeze or atomic Developer Instructions size refusal through host event
callbacks. Implement and qualify those contracts in the owning widget/input dependencies before
resuming the complete App mount. No Settings service or window implementation is claimed.
