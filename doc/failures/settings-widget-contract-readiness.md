# Settings Widget Contract Readiness

## Invalidated Assumption

The Settings mounting readiness audit treated the owned widget's documented field availability,
window-wide reconciliation gate and bounded input behavior as live production prerequisites.
The proposed App-only mount cannot satisfy those contracts at the pinned dependency revisions.

## Evidence

Beryl pins `gpui-settings-window` to `feb598064dcc6b5ea85ba7fad02173f3dd8f2109` and
`gpui-text-input` to `57d51db735890d5cc53ddc6f73feb2f5563f8e92`. At the Settings pin:

- `src/model.rs::SettingsWindowModel` contains sections and selected section/page identities;
  it supplies no window-wide interaction or footer gate.
- `src/panel/render.rs::render_buttons` passes `enabled = true` for OK, Apply and Cancel.
- `src/window.rs::handle_window_close_requested` hides before emitting `CloseRequested`.
- `src/input.rs::text_input_options` supplies only input shape and undo retention budget;
  `FieldChanged` observes an already mutated input. The text-input options have no edit byte cap.

Host rejection after an event cannot preserve the original text, caret, selection and undo state
or prevent native hiding. Reconnecting the orphan old Settings implementation does not fix this.

## Correction

Implement the existing interaction/availability contracts in the owned Settings widget and add
an explicit pre-mutation bounded input contract at its owning input boundary. Qualify all input
routes and native close/footer/navigation gating, publish the exact dependency revisions, and
only then resume the App mount. Track the prerequisite in [the active plan](../plan.md).
Distinguish target documentation from implemented API evidence in future readiness reviews.

The App ownership design remains authoritative. The exact orphan Settings sources were archived
before discovery; they were already outside the live module graph. The direct owning-input/widget
correction is now independently qualified and published; [evidence](../audits/settings-widget-prerequisites.md).
The App service/window mount remains a separate acceptance boundary.
