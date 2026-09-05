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
establish this boundary independently of ordinary shell integration.

## Accepted Shell Integration

Phase 289 completed the ordinary shell in `main_window/shell.rs` and its `host`, `appearance`, and
`custody` modules. Preparation verifies exact home, runtime, root, claim, and editor facts off GPUI.
Hidden roots register with the shared appearance owner and require the current generation and exact
first-presentable editor before native publication. The complete composer mount retains its ordinary
subscriptions and autosave initialization. Empty optional mounts consume no space.

Review invalidated three incomplete integration checks. Matching natural acquisition identifiers
cannot establish home provenance: the token now retains its home, and both preparation and cleanup
reject a foreign token before local mutation. A retained composer contribution does not prove its
editor is mounted: final appearance validation checks the canonical owner lifecycle before the
whole-set adoption cut. Panel padding alone is not a usable window minimum: minimum geometry now
includes the prepared editor's configured text and scrollbar allocation. Focused regressions cover
all three corrections, and independent semantic review accepted the result.

GPUI entity construction is infallible, but a clean fallible Beryl mount factory is available:
prepare the contribution, construct the ordinary complete entity, then initialize subscriptions
and autosave through entity update, dropping a failed entity and returning unpublished custody.
An inert error-bearing placeholder mount is unnecessary and was not implemented.

`cargo check -p beryl-app --lib --features test-faults --locked` passed. The final gate was:

```text
cargo nextest run -p beryl-app --test phase289_main_window_shell --test main_window_reservations --test phase285_selected_composer_preparation --test phase238_window_abandonment --test phase236_window_acquisition --features test-faults --locked --test-threads 1 --no-fail-fast --status-level pass
```

All 47 tests passed with none skipped (run `cb0d46b7-ba9d-4926-a3a0-6415df852978`): nine shell,
five reservation, six selected-preparation, eight abandonment, and nineteen acquisition cases.
The Windows run used process-scoped `RUST_MIN_STACK=16777216` for existing populated preparation
fixtures; the prior environment was restored afterward. Real GPUI cases verify distinct controllers
and editors, current preview and later atomic adoption, preserved editor state, released-editor
rejection, usable minimum layout, post-native construction failure, and unready or stale selection
refusal before visibility. Custody tests verify unresolved cleanup retains the reservation and exact
terminal settlement releases it.

The integration includes the previously accepted reservation and selected-preparation working
material. No task-owned build process or permanent environment change remained at handoff. Shared
build artifacts and prior retained neutral checkouts were left untouched. Binary bootstrap and
ordinary New Window command integration remain later plan boundaries.
