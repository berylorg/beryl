# Reason For Investigation

Primary New Thread tests could focus both split-button segments directly, but actual Tab
navigation skipped them despite `Div::tab_stop(true)`. The investigation checked how a tracked
focus handle enters GPUI's tab order.

# Outcome

At this revision, Div applies its tab-stop setting only when it creates an internal focus
handle. An explicitly tracked handle bypasses that construction path. Element-level tab-stop
configuration therefore does not make an existing tracked handle a tab stop.

Configure the owned handle itself with `FocusHandle::tab_stop(true)` before tracking it. That
method updates both the handle and its registered focus record. For independently focusable
split-button segments, configure both owned handles and verify real Tab and Shift+Tab traversal
through rendered order; direct focus calls do not establish keyboard reachability.

Refresh this finding when the GPUI pin or focus construction changes. No fork change is needed
for this integration behavior.

# Sources

- Canonical remote: `https://github.com/berylorg/zed-fork.git`; full resolved commit:
  `edd4928c5be424630da49f872e00dafbf94cf0b2`, selected by the Beryl root `Cargo.toml`.
  Accessed 2026-10-08 by focused symbol and source inspection for Windows virtual-GPUI fixtures.
- [`crates/gpui/src/elements/div.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/elements/div.rs):
  `request_layout` applies tab-stop and tab-index settings only to internally constructed handles.
- [`crates/gpui/src/window.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/window.rs):
  `FocusHandle::tab_stop` updates the handle and its registered focus record.
