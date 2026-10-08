# Reason For Investigation

New Thread failed-home recovery qualification reached resident preparation in a virtual
GPUI window but kept reporting that the previous resident frame had not returned. The
investigation checked whether drawing the virtual window delivered its queued frame callbacks.

# Outcome

At this pinned revision, `TestWindow::on_request_frame` accepts and discards the platform
callback, and its platform `draw` is empty. `Window::on_next_frame` queues callbacks. The
platform request-frame closure installed by `Window::new` drains that queue and invokes
each callback through the actual window handle before drawing. Calling `Window::draw`
alone renders and swaps buffers; it does not drain the callback queue.

Beryl's resident preparation callback owns a frame wake through its deferred application
callback. The retained weak wake correctly refuses another advance until that original
callback has returned. Manually advancing preparation leaves the queued callback alive and
therefore cannot reproduce production frame delivery.

The focused Beryl fixture captures the exact original callback in an opt-in test-only slot,
takes it once, and invokes it after releasing the slot borrow. Its ordinary deferred callback
then runs and releases the original wake. Native scheduling remains unchanged. This supports
real preparation and retirement qualification without weakening the resident wake guard or
claiming native platform coverage. Refresh this finding if the GPUI pin or virtual platform
request-frame implementation changes.

# Sources

- Canonical remote: `https://github.com/berylorg/zed-fork.git`; full resolved commit:
  `edd4928c5be424630da49f872e00dafbf94cf0b2`, selected by the Beryl root `Cargo.toml` GPUI pin.
  Accessed 2026-10-08 by focused source inspection.
- [`crates/gpui/src/platform/test/window.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/platform/test/window.rs):
  `TestWindow::on_request_frame` and `draw` establish the virtual platform limitation.
- [`crates/gpui/src/window.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/window.rs):
  `Window::new` request-frame closure, `on_next_frame`, and `draw` establish queue ownership
  and the actual delivery path.
- Beryl use site: `crates/beryl-app/src/main_window/conversation_composer_owner/resident.rs`,
  `schedule_resident_preparation`; test-only delivery support in
  `crates/beryl-app/src/main_window/conversation_composer_owner/running_owner.rs`.
