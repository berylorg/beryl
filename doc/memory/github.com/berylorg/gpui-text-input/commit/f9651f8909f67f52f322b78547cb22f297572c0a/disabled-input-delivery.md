# Reason For Investigation

Primary New Thread fences an already-open root picker's search without changing text, selection,
IME composition, history or scroll. Independent review checked whether disabling the owned-value
input rejects native callbacks already registered by its previous paint.

# Outcome

At this TextInput revision, disabling removes input routing during the next paint. Native
replacement, marked replacement and unmark callbacks do not themselves check the enabled flag;
editing actions also lack delivery-time guards. GPUI Windows character and IME handlers invoke
the retained input handler directly, and ElementInputHandler forwards those calls without an
additional enabled check. A queued callback before repaint can therefore mutate disabled input.
Existing selection-drag and multiline wheel callbacks also lacked current enabled admission and
could change selection or scroll after disabling.

Keyboard capture is insufficient: keyboard simulation exercises the key-dispatch entrance,
whereas queued native callbacks and retained actions reach their mutators directly. Qualify
those exact callbacks with enabled positive controls and disabled complete-state preservation.
Admission belongs at the owned input's delivery boundary. Do not repair rejected edits by
resetting text after its Changed event, which discards caret, history and composition state.

Refresh this finding after either dependency pin changes. The bounded correction preserves
already admitted range-backed operation settlement and cleanup.

# Sources

- Canonical remote: `https://github.com/berylorg/gpui-text-input.git`; resolved commit
  `f9651f8909f67f52f322b78547cb22f297572c0a`, selected by Beryl's root `Cargo.toml`.
  Accessed 2026-10-08 by focused source inspection and independent review.
- [`src/widget/ime.rs`](https://github.com/berylorg/gpui-text-input/blob/f9651f8909f67f52f322b78547cb22f297572c0a/src/widget/ime.rs):
  owned-value EntityInputHandler replacement, marked replacement and unmark entry points.
- [`src/widget/keyboard.rs`](https://github.com/berylorg/gpui-text-input/blob/f9651f8909f67f52f322b78547cb22f297572c0a/src/widget/keyboard.rs):
  editing, selection, clipboard, pointer-drag and wheel delivery handlers.
- Canonical GPUI remote: `https://github.com/berylorg/zed-fork.git`; resolved commit
  `edd4928c5be424630da49f872e00dafbf94cf0b2`, verified with exact-revision source reads.
- [`crates/gpui/src/platform/windows/events.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/platform/windows/events.rs):
  character, dead-character and IME delivery through the currently registered input handler.
- [`crates/gpui/src/input.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/input.rs):
  ElementInputHandler forwards native replacement and composition calls to its entity.
