# Main-Window Notice Readiness

## 2026-09-06: Existing Detail And Theme Boundaries Do Not Complete The Widget Contract

Phase 300 readiness inspection invalidated unchanged reuse of the owned-value `TextInput` as the
notice's complete selectable-detail implementation. This is source evidence; no GPUI behavior test
or production implementation was performed in this slice.

The pinned `gpui-text-input` revision `fc17c5738c35350e58e32437cdae74e28ebc31af` matches the inspected
sibling checkout. Its [widget contract](../../../gpui-text-input/doc/gui/widgets/text-input/spec.md)
and [package design](../../../gpui-text-input/doc/design.md) require disabled instances to install
no input routes or hitbox and emit no widget input events. In `src/widget/render.rs`, however,
`TextInput::render` gates only focus/key-context/tab/cursor setup: action, pointer, wheel, and
scrollbar routes remain installed. In `src/widget/keyboard.rs`, `copy` writes the clipboard and
emits `CommandHandled`, and `on_scroll_wheel` changes the scroll offset, without an enabled guard.
`set_enabled(false)` therefore does not establish the notice's inert boundary. This is a dependency
implementation defect against existing authority, not permission to weaken inert behavior.

Separately, `src/widget/mod.rs::TextInput::set_text` resets both scroll offsets and requests cursor
reveal. The inspected owned-value public API exposes `scroll_offset` but no offset restoration
setter or shared scroll handle; selection setters request cursor reveal. Unchanged use of
`set_text` cannot preserve valid top-visible text geometry when the same notice identity receives
newer detail, as the [notice spec](../gui/widgets/main-window-notice/spec.md) requires.

The finite Beryl theme inventory in `crates/beryl-state/src/theme/schema.rs` contains
`notice.title`, `notice.detail`, and severity surface roles, but no `main-window-notice` family.
The [theme runtime contract](../systems/theme-runtime/design.md) requires canonical widget-owned
role ids. The notice spec's anatomy, state, severity, and layout roles need schema coverage before
the widget can consume them through complete resolved appearances.

The plan now separates canonical theme-role implementation as Phase 303 and keeps Phase 300
pending. The notice spec references command buttons and the external scrollbar; it does not require
`text-input`. Widget-local readonly mechanics are permitted in principle, but this investigation
did not establish an accepted complete detail primitive. Resolve that boundary explicitly before
activation; do not conceal the dependency defects with input interception or claim scroll
preservation by resetting it. Any independent dependency repair or new component needs its own
bounded phase and applicable authority.

Accepted command/focus examples exist in
`crates/beryl-app/src/main_window/creation/command.rs` and
`crates/beryl-app/src/main_window/conversation_composer_mount/native_lineage/prompt.rs`.
`crates/beryl-app/tests/phase290_main_window_creation/gpui.rs` and
`crates/beryl-app/tests/theme_runtime_cases/production/gpui_fixture.rs` provide rendered-input and
appearance verification seams. Future widget acceptance still needs exact input rejection,
replacement/focus, selection/copy, revision-sensitive scrolling, bounded geometry, diagnostics
privacy, and actual themed rendering evidence.
