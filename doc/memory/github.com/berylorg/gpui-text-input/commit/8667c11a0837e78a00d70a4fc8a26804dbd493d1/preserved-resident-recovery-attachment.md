# Reason For Investigation

Interrupted-Exit recovery reconstructs a clean composer host in a fresh home generation while
preserving its native window and resident presentation. Can the pinned widget attach that host
while retaining the old coherent surface until caret, directed selection, scroll and history are
coherently available under the fresh binding?

# Outcome

The inspected public APIs do not provide that complete attachment boundary. Existing direct
rebinding retains a paint-only old surface while realizing a successor, but does not transfer the
full restoration state. Restoration import and prepublication construction serve different cuts.
This is a bounded missing widget capability, not evidence that preserving the resident is
impossible. No widget code was changed and no runtime qualification was performed.

- Beryl's reconstructed host advances the host generation. `ComposerHostBinding::range_binding`
  uses that generation as `BindingId`; retaining the same draft revision and extent therefore
  still produces a different widget binding. The existing app
  `synchronize_lifecycle_selection` requires equal range bindings and cannot perform this handoff.
- `RangeTextInput::rebind` accepts a binding and optional directed selection, but no exact scroll
  anchor or history frontier. `prepare_rebind_transition` initializes a new desired surface from
  origin, targets the selection head and requests caret reveal. The committed direct rebind sets
  history to unavailable. It therefore does not, on its own, preserve the exported recovery seed.
- A direct rebind admits a bounded pending intent before trying to service it. `Busy` can leave
  that intent retained, and service errors schedule a continuation. Callers cannot treat every
  error as an unchanged widget or independently retry another attachment without settling custody.
- `set_history_frontier` requires both expected and replacement frontiers to match the widget's
  current binding. It is useful after a binding transition, but neither rebinds the widget nor
  transports the exact scroll anchor through that transition.
- `import_restoration` requires full quiescence and equality between seed and current binding.
  Admitted import resets residency and geometry, clears the coherent surface, releases adopted
  prepublication custody and starts fresh bounded validation. Thus importing the old seed into
  the old widget does not attach the new binding; importing a translated seed after ordinary
  rebind is not a preservation boundary either. It first needs rebind work to drain and then
  clears the surface. The existing restoration-failure test explicitly expects no surface.
- Non-mounted prepublication already realizes one exact seed through bounded fresh source reads,
  owner validation and window-affine geometry. Its public `new_with_prepublication` adoption
  constructs a new widget; it does not replace the publication of an existing resident widget.
  Its source/history/environment/capacity checks and explicit cleanup ledger are relevant reusable
  mechanisms, but a constructor is not evidence of same-resident adoption support.
- Beryl's `RecoveryFenced` resident cannot pump ordinary requests. A recovery implementation
  therefore needs explicit fresh-source progress and completion custody while interaction remains
  fenced; merely assigning a fresh service cannot drive the existing restoration protocol.

A likely implementation direction is a bounded same-resident coherent adoption boundary using the
existing prepublication machinery. That is a recommendation, not an accepted dependency contract.
Before implementation, the widget authority must specify exact predecessor and successor
validation, preservation of the resident/focus and old paint on refusal, frozen environment and
history checks, combined old/new capacity admission, one-shot adoption and cleanup ownership.
The app must independently prove unchanged durable content/history before preparing fresh binding
facts. The widget cannot infer that equality from matching offsets or revisions alone. Source
dispatch, whole-graph publication and final gate release remain separate app obligations.

# Sources

Inspected on 2026-09-28 in
[berylorg/gpui-text-input](https://github.com/berylorg/gpui-text-input), commit
`8667c11a0837e78a00d70a4fc8a26804dbd493d1`. Beryl's workspace manifest pins this revision;
its Windows local Cargo patch selects the sibling checkout at the same commit. Tracked widget
source was clean. No extra widget features are enabled by the app.

- `src/range_widget/lifecycle.rs`: `rebind`, `commit_pending_direct_rebind`,
  `export_restoration`, `import_restoration`.
- `src/range_widget/realization/rebind.rs`: `retain_pending_rebind_intent`,
  `service_pending_rebind_intent`.
- `src/range_widget/transition.rs`: `prepare_rebind_transition`.
- `src/range_widget/history.rs`: `history_frontier`, `set_history_frontier`.
- `src/range_widget.rs`: `is_quiescent`.
- `src/range_widget/prepublication/adoption.rs`: `new_with_prepublication`.
- `doc/design.md`: Range-Backed Prepublication Realization and compact restoration contracts.
- `tests/range_widget.rs`: `retained_prior_surface_is_paint_only_after_rebind` and
  `restoration_mismatch_failure_and_lifecycle_cancel_release_exact_work`, inspected but not run.

Local Beryl use sites at commit `3057dd00`: `composer_host/model.rs`,
`composer_host/lifecycle/retirement.rs`, and
`main_window/conversation_composer_owner/{lifecycle,recovery}.rs` under
`crates/beryl-app/src`. Interpretation follows the app catalog/composer and shell-lifecycle
supplements, composer feature, and backend-runtime interrupted-Exit recovery contract.

Resolution used manifest/local-patch inspection, `git rev-parse HEAD`, `git status --short`,
`git remote get-url origin`, targeted Serena symbol bodies and bounded public-API searches.
Refresh when the pin, adoption/restoration APIs, host binding mapping or resident recovery fence
changes. This note supplements the sibling ownership investigation; neither establishes runtime
recovery acceptance.
