# Theme Runtime GPUI Publication

## Invalidated Assumption

Phase 289 assumed the accepted appearance coordinator and range-input live-appearance capability
were sufficient to mount an ordinary theme-aware shell. The coordinator had only content-oriented
test adapters; no production GPUI publication boundary connected its success to actual root and
mounted-control adoption. Standalone shell colors, render-time polling, or a separate caller
adoption step after coordinator success cannot satisfy atomic publication.

## Decisive Evidence

At the Phase 293 diagnosis boundary:

- `crates/beryl-app/src/theme_runtime/adapter.rs` prepares adapters from an immutable generation;
  the prepared adapter's commit receives no GPUI context.
- The free `commit_adapters` function in `theme_runtime/publication.rs` invokes adapter commits;
  its callers advance coordinator state and report success afterward, without an actual GPUI
  window-set adoption owner.
- `ThemeRuntime::consume_change_hints_inner` reaches `refresh_repository` or
  `reload_active_document`, then `publish_external`. Settings outcome consumption, durable retry,
  preview publication, and Stop Preview also reach coordinator commits internally. A shell-local
  registration wrapper cannot intercept every later publication.
- Repository paths combine blocking preparation and publication. Running whole runtime operations
  on GPUI would violate the existing prohibition on file access, parsing, hashing, and resolution
  there. The accepted dependency method `RangeTextInput::set_appearance` is synchronous, so the
  missing boundary is in Beryl integration rather than dependency capability.

Independent semantic review confirmed that this is a separately implementable and verifiable
prerequisite. The theme-runtime system's Appearance Generation Publication, Tool And Worker
Boundaries, Bounds And Diagnostics, and recovery sections, together with the app's Settings And
Themes adapter contract, already settle the required behavior. No new product contract is needed.
Phase 293 completion review accepted this diagnosis and the Phase 294 prerequisite ordering only.

## Correction

Phase 294 replaced per-adapter coordinator dispatch with one bounded app-owned GPUI window-set
publication boundary in `theme_runtime/gpui_publication.rs`. Every existing appearance source uses
that target. Workers retain finite appearance facts and receive acknowledgement after actual
root and mounted-control adoption; repository preparation remains off GPUI. Registration supplies
the exact current appearance, and window changes invalidate captured epochs.

Adapter callbacks must not run under the mailbox mutex: reentrant snapshot or retirement would
deadlock GPUI and strand the waiting worker. All adapters prepare outside that lock, every prepared
root passes final validation, and the exact active/current/epoch checks precede the whole-set
adoption cut. Retirement before that cut rejects the attempt; retirement after it orders after
the complete infallible adoption. Retirement itself does not synchronously wait for adoption.
Closing an earlier root during a later adapter's preparation rejects before any surviving root
changes. These corrections passed independent semantic review.

`cargo check -p beryl-app --lib --features test-faults --locked` passed. The final
`cargo nextest run -p beryl-app --test theme_runtime --features test-faults --locked --test-threads 1
--no-fail-fast --status-level pass` run passed all 33 tests with none skipped (run
`c8e74459-39d9-428d-983a-ea0738fc20fd`). Five actual GPUI tests in
`tests/theme_runtime_cases/production/gpui.rs`, with `gpui_fixture.rs`, cover root and composer
scene colors, unchanged editor state, all appearance sources, rejection, reentrant callbacks,
pending capacity, creation/removal epoch invalidation, and retirement release. Minimal test roots
establish this boundary independently; Phase 289 still must integrate the ordinary shell.

## Retained Shell Checkpoint

Phase 289 remains unaccepted. Its working material is retained, not included in the diagnosis
commit:

- `crates/beryl-app/src/main_window/shell.rs` and its export in `main_window/mod.rs`.
- `crates/beryl-app/tests/phase289_main_window_shell.rs`.
- Narrow constructor integration in `main_window/conversation_composer_mount.rs` and appearance
  forwarding in `main_window/conversation_composer_owner/lifecycle.rs`.

The worker reported that `cargo check -p beryl-app --test phase289_main_window_shell --features
test-faults` passed. No nextest result is accepted: the invocation ended but its result was not
retained. Initial review found required corrections for full acquisition/editor identity,
coordinator-owned appearance, canonical adaptive sizing and minimum window geometry, readiness
before native publication, and reuse of the complete composer mount. The constructor and readiness
changes have not received completion review; unresolved abandonment/reconciliation, distinct
controller ownership, production construction failure, stale selection, and appearance tests remain.

GPUI entity construction is infallible, but a clean fallible Beryl mount factory is available:
prepare the contribution, construct the ordinary complete entity, then initialize subscriptions
and autosave through entity update, dropping a failed entity and returning unpublished custody.
An inert error-bearing placeholder mount is unnecessary and was not implemented.

No task-owned build process remained at the handoff. Existing unrelated build processes and the
previous phase's retained neutral checkouts were left untouched. Resume only the recorded paths and
acceptance boundaries; do not infer passing tests or finished shell behavior from compilation.
