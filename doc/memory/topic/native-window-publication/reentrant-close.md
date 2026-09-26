# Reason For Investigation

Determine whether a startup callback can veto native close while GPUI publishes a borrowed window.

# Outcome

The registered callback wrapper invokes `AsyncWindowContext::update` and defaults errors to `true`.
That update can fail because the app is already borrowed or the exact window is temporarily taken
by an outer update. A synchronous Windows close during publication can therefore bypass the app
callback. Denying this request on update failure needs no new API or retained state; the subsequent
ordinary request can execute after the borrow ends. An absent registered callback remains a
separate platform default. The Windows handler temporarily takes a callback during its execution,
so this correction does not promise arbitrary recursive close calls from inside that callback;
startup admission is a side-effect-free check.

Existing editor enabled/read-only controls can gate text mutation, including native text input.
`interactive_surface()` tests coherence rather than enabled state, so disabling does not itself
invalidate first-presentable readiness. Disabling can reject its internal transition and requires
checking the resulting state. Composer lifecycle promotion/resume and propagated commands still
need an app-owned gate; the close correction alone does not provide complete-set admission.

Enabling a disabled editor prepares a local active-object transition even when no object changes.
Its ownership-plus-transition charge can exceed configured capacity, so a void setter return is
not proof of success. Successful enabling and read-only changes update local fields and enqueue
notifications; they make no native call or synchronous user callback. GPUI flushes queued effects
only after the outermost app update. Consequently a single outer window update can enable and
verify every supplied member while read-only/command gates remain closed, then release admission.
On a later rejection it can attempt regating the prefix before returning; any disabling refusal
must remain explicit. Separate awaited GUI updates would not provide the same no-event gap.

# Sources

- Owned GPUI revision `11e7d5c41d06f6378ec036fd881c7eb18011f0e7`, inspected 2026-09-26:
  `crates/gpui/src/window.rs` (`on_window_should_close`, `publish`), `app/async_context.rs`
  (`AsyncApp::update_window`), `app.rs` (`update_window_id`), and
  `platform/windows/events.rs` (`handle_close_msg`).
- Owned text-input revision `aaf4a80dc3f1f06f4ca7574eadfea4bbc05dea61`, inspected 2026-09-26:
  `src/range_widget.rs` (`set_enabled`, `interactive_surface`),
  `src/range_widget/transition.rs` (interaction preparation/commit) and native input handlers.
  Revision `8ae854b9ebe5beb4885860cf4f7d95b68a20bbb7` changes dependency pins only.
- Beryl revision `5e0cbb3c`, shell host and composer construction/lifecycle/close paths,
  inspected 2026-09-26. Durable requirements belong to the shell-lifecycle design supplement.
