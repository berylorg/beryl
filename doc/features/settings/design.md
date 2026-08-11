# Goals

Give users a durable, validated settings window for Beryl-owned application preferences without exposing backend-owned Codex configuration or mixing feature semantics into generic settings mechanics.

## Non-goals

- Providing settings UI for backend-owned Codex authentication, session storage, skills, MCP, or transcript history.
- Owning the semantics of every setting exposed in the window.
- Live-previewing ordinary unapplied settings drafts.
- Owning appearance theme schema, theme repository, or theme editor behavior.

# Decisions

## GUI Supplement

- [`gui.md`](gui.md) is a normative supplemental GUI composition file for settings toolbar entry, settings window shell layout, sections, rows, and settings-window widget usage.
- Durable settings commit and same-home reconciliation mechanics are defined in
  `doc/systems/beryl-home-storage/design.md`; the typed supported-setting storage boundary is
  defined in `crates/beryl-state/doc/design.md`.

## Settings Window

- Application settings live in a dedicated top-level settings window, not an in-place modal or main-conversation panel.
- The main conversation toolbar exposes a Settings control that opens or reveals the dedicated
  settings window immediately with the latest coherent settings presentation. A slower
  feature-owned page may show localized loading or unavailable state without delaying the window.
- The settings window does not include the main conversation toolbar strip.
- It uses a left sidebar of broad sections and one right-pane page or subpage at a time.
- Sidebar rows do not expand into nested trees. Subpages open in the right pane with breadcrumb/back navigation while the sidebar remains at the broad section level.
- Current V1 sections are `Themes`, `Operations`, `Notifications`, and `Agent`.
- The `Themes` section's product behavior is owned by `doc/features/theming/design.md`.
- Unavailable sections, invalid staged values, failed saves, and failed feature-owned settings operations render localized page or row feedback without replacing the settings shell.

## Color-Valued Settings

- Color-valued settings expose canonical `#rrggbb` text, a preview swatch for the latest valid
  color, and an in-window color picker from the swatch or a field hotkey.
- If a color text draft is temporarily invalid, the preview swatch and picker channel values continue to use the latest valid color for that setting until a new valid color is staged.

## Startup Values And Availability

- A supported setting with no saved value starts at its owning feature's declared default and is
  presented as clean rather than as an unsaved modification.
- A saved value that is invalid or rejected by its owning feature never becomes active. The affected
  row or page shows localized unavailable feedback and the declared default remains the active
  value; Beryl does not silently claim that the rejected value loaded successfully.
- When no coherent settings snapshot can be presented, the settings window shell still opens and
  shows the affected pages as unavailable. It does not fabricate editable values or block unrelated
  feature pages whose coherent state is known.
- A setting removed from the current product has no row, fallback control, or compatibility page.
  Its former saved value is never applied to another setting.

## Drafts, Validation, And Persistence

- The settings window maintains one window-wide staged draft with shared modified, reset, discard,
  commit, and visible outcome behavior. Each feature that owns a setting defines its default,
  meaning, accepted domain, and domain validation.
- GUI-owned user settings are persisted separately from backend-owned Codex configuration.
- Operation preferences, notification preferences, developer-instructions preferences, and AI-control preferences are app-wide Beryl-home settings.
- Applying settings validates the complete modified draft before any value becomes active. One
  Apply or modified OK action commits every modified setting together or commits none of them.
- Navigating between sidebar sections, pages, or subpages preserves the complete staged draft,
  modified indicators, and validation feedback. Navigation alone never applies, resets, or discards
  a setting.
- Reset on a modified row restores that row to its current proven active value and clears only that
  row's staged modification and draft-local validation feedback. If no coherent active value is
  available for the row, Reset is unavailable; Cancel remains the whole-draft discard command when
  no reconciliation is active.
- Apply is enabled only when at least one row is modified, every modified row is available and
  valid, and no settings commit is reconciling. With no modified rows, Apply is disabled and OK may
  hide the window without issuing a commit.
- When modified rows contain any invalid or unavailable value, Apply and OK are disabled and each
  blocking row or page presents localized feedback. Valid modified rows remain staged, but Beryl
  never commits that valid subset separately from the invalid or unavailable remainder.
- The settings-window footer retains `OK`, `Apply`, and `Cancel`. `Apply` performs that operation
  and leaves the window open. `OK` performs the same operation and hides the window only after a
  complete durable commit is proven.
- Validation or persistence failure keeps the settings window open, preserves its staged draft, and reports the exact failure without partially accepting the update.
- An indeterminate Apply or OK outcome keeps the window open on its current page with the exact
  within-cap staged draft and last coherent active values visibly reconciling. Field edits, Reset,
  navigation, Apply, OK, and Cancel are unavailable until the outcome resolves; content may remain
  readable, selectable, scrollable, and copyable.
- Ordinary settings-window close and application Exit do not complete or queue a later close while
  reconciliation is active. Repeated Apply, OK, close, or Exit activation cannot duplicate the
  update. The user must invoke close or Exit again after controls are re-enabled.
- If reconciliation proves the complete commit, all accepted values become active together and the
  matching draft becomes clean. Apply leaves the window open; OK then hides it. Normal editing,
  navigation, Reset, Cancel, close, and Exit behavior is re-enabled.
- If reconciliation proves non-commit, the prior active values remain and the exact staged draft is
  restored with failure feedback. Controls are re-enabled, and a later Apply or OK is a new explicit
  attempt rather than an automatic retry.
- If reconciliation cannot prove either complete outcome, only the affected settings rows show an
  unresolved outcome. Those rows retain their last coherent presentation with localized feedback
  but are unavailable for editing, Reset, or another commit; Beryl does not claim that either the
  old or intended new value is active.
- After that scoped unresolved outcome, navigation, unrelated healthy rows, Cancel, ordinary window
  close, and application Exit are re-enabled. Apply and OK remain unavailable while the preserved
  draft contains affected modifications. Cancel or close may discard the complete local draft and
  hide the window, and Exit may proceed normally; none resolves the affected durable state or
  authorizes a duplicate commit. On a later same-process opening, the affected rows remain
  unavailable while unrelated valid rows may form and commit a new draft normally.
- Only structural Beryl-home failure or reopening uses the shared home-failure presentation and
  makes settings that require the home unavailable. A scoped unresolved settings outcome never
  escalates to that presentation merely because it remains unresolved.
- Ordinary `Cancel` and settings-window close discard the complete unapplied draft and hide the
  window without changing active settings when no reconciliation is active.
- If the process is forcibly terminated while an indeterminate outcome is reconciling or remains
  scoped unresolved, the window-local staged draft is lost and is not restored on restart. Normal
  startup loads whichever complete valid settings snapshot is durably present and opens a clean
  draft without claiming proof of the prior operation's old or intended new outcome. If startup
  cannot establish a coherent snapshot, the ordinary startup unavailable behavior above applies.
- Ordinary settings drafts do not live-preview unapplied changes. User-visible theme Preview behavior is owned by `doc/features/theming/design.md`.

## Feature-Owned Settings Rows

- Settings rows share staged-value, modified, availability, commit, and Reset behavior. The feature
  that owns a setting defines its semantics, default, domain validation, and feature-specific
  actions.
- The Operations section includes `Context compaction timeout`; its selected-thread compaction semantics are owned by `doc/features/status-line/design.md`.
- The Operations section includes `Draft autosave interval`; its required 5-through-300-second range and 30-second default are owned by `doc/features/composer/design.md`.
- The Agent section includes `Developer Instructions`. A saved value accepts at most 60 KiB of
  UTF-8, while the editor and window-wide staged draft impose a 64 KiB UTF-8 hard cap for this
  field. Values above 60 KiB through 64 KiB remain exact, visibly invalid drafts that can be edited
  down; Apply and OK remain disabled.
- Any typing, paste, drop, replacement, or IME commit whose resulting Developer Instructions value
  would exceed 64 KiB is rejected atomically before editor mutation. Beryl inserts no prefix or
  truncation and preserves the prior within-cap text, caret, selection, and undo state with
  row-local feedback. Exact draft preservation is promised only through the 64 KiB cap.
- Send-time Developer Instructions behavior is owned by `doc/features/composer/design.md`.
- The Notifications section includes `End-turn sound`; notification playback semantics are owned by `doc/features/notifications/design.md`.
- The Themes section and theme editor are owned by `doc/features/theming/design.md`.
- Feature-owned rows must keep controls reachable and labels readable at supported minimum settings-window width.
