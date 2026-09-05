# Main-Window Notice Readiness

## 2026-09-06: An Optional Text-Control Candidate Was Mistaken For A Widget Blocker

The [notice contract](../gui/widgets/main-window-notice/spec.md) requires bounded selectable
readonly detail. Notifications contain no editable text input. Its direct dependencies are command
buttons and the external scrollbar; the widget owns detail selection, scrolling, and inert behavior.

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

The inspected Beryl theme inventory in `crates/beryl-state/src/theme/schema.rs` contained
`notice.title`, `notice.detail`, and severity surface roles, but no `main-window-notice` family.
The [theme runtime contract](../systems/theme-runtime/design.md) requires canonical widget-owned
role ids. Phase 303 added the 20 supported color and typography roles, verified exact fallbacks and
severity overrides through the canonical resolver, and passed independent semantic review. The
existing theme values support RGB colors and typography; widget-local geometry, transparent paint,
shadow, and opacity retain the widget spec's rendering defaults rather than expanding that model.

These findings reject optional unchanged `TextInput` reuse. They do not establish a notification
architecture gap or make repairing `TextInput` a prerequisite. Pausing Phase 300 on that basis
incorrectly promoted an implementation candidate into a required dependency. Implement the
notice-owned readonly detail mechanics under the existing widget authority; do not introduce
editable fields or dependency repair solely to preserve that candidate. The source findings remain
unverified by behavior tests and do not authorize unrelated dependency work. Phase 300 can consume
the accepted canonical theme roles and implement its own bounded selectable detail.

Accepted command/focus examples exist in
`crates/beryl-app/src/main_window/creation/command.rs` and
`crates/beryl-app/src/main_window/conversation_composer_mount/native_lineage/prompt.rs`.
`crates/beryl-app/tests/phase290_main_window_creation/gpui.rs` and
`crates/beryl-app/tests/theme_runtime_cases/production/gpui_fixture.rs` provide rendered-input and
appearance verification seams. Future widget acceptance still needs exact input rejection,
replacement/focus, selection/copy, revision-sensitive scrolling, bounded geometry, diagnostics
privacy, and actual themed rendering evidence.
