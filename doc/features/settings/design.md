# Goals

Give users a durable, validated settings window for Beryl-owned application preferences without exposing backend-owned Codex configuration or mixing feature semantics into generic settings mechanics.

## Non-goals

- Providing settings UI for backend-owned Codex authentication, session storage, skills, MCP, or transcript history.
- Owning the semantics of every setting exposed in the window.
- Live-previewing ordinary unapplied settings drafts.
- Owning appearance theme schema, theme repository, or theme editor behavior.

# Decisions

## GUI Supplement

- `gui.md` is a normative supplemental GUI composition file for settings toolbar entry, settings window shell layout, sections, rows, and settings-window widget usage.

## Settings Window

- Application settings live in a dedicated top-level settings window, not an in-place modal or main-conversation panel.
- The main conversation toolbar exposes a Settings control that opens or reveals the dedicated settings window.
- The settings window is created ahead of first use and hidden when inactive so opening it feels immediate.
- The settings window does not include the main conversation toolbar strip.
- It uses a left sidebar of broad sections and one right-pane page or subpage at a time.
- Sidebar rows do not expand into nested trees. Subpages open in the right pane with breadcrumb/back navigation while the sidebar remains at the broad section level.
- Current V1 sections are `Themes`, `Operations`, `Notifications`, and `Agent`.
- The `Themes` section's product behavior is owned by `doc/features/theming/design.md`.
- The settings window root layout stretches with the OS window. The sidebar has a bounded fixed logical width; the main pane takes remaining width.
- Page content is organized as section headings followed by grouped row lists. Pages must not nest cards inside cards.
- The settings shell itself is not an outer scrolling container. The sidebar and current page body own their own scrolling while headers and apply/action areas remain reachable.
- Unavailable sections, invalid staged values, failed saves, and failed feature-owned settings operations render localized page or row feedback without replacing the settings shell.

## Settings Rows

- Settings rows are schema-backed key/value rows with stable setting ids or action ids, label, optional description, value/control area, modified state, and context actions.
- Row value controls use type-appropriate widgets such as switches, segmented controls, dropdowns, steppers, sliders, text fields, multiline fields, color inputs, file path pickers, action buttons, or step-in affordances.
- Labels and descriptions wrap before controls shrink below useful widths.
- Numeric fields use compact widths sized for short numeric values.
- Step-in rows use a right-facing triangle affordance and navigate the right pane to a subpage.
- Modified rows expose reset when reset is valid.
- Apply, Revert, Save, Save As, Install Theme, and similar feature-owned actions belong to settings-window chrome, page headers, row value areas, or page-level action areas, not the main conversation toolbar.
- Color-valued settings use a dedicated color input with canonical `#rrggbb` text, a preview swatch for the latest valid color, and an in-window color picker from the swatch or a field hotkey.
- If a color text draft is temporarily invalid, the preview swatch and picker channel values continue to use the latest valid color for that setting until a new valid color is staged.

## Drafts, Validation, And Persistence

- Beryl owns settings schemas, validation, staged drafts, apply behavior, and persistence.
- GUI-owned user settings are persisted separately from backend-owned Codex configuration.
- Operation preferences, notification preferences, developer-instructions preferences, and AI-control preferences are app-wide Beryl-home settings.
- Explicit scalar GUI preferences are stored in the Beryl settings keyspace under the configured Beryl home.
- Settings update operations commit through validation, active-update, persistence, and recovery paths used by settings-window Apply.
- Applying settings validates staged values before they become active, updates the running UI, and persists accepted settings without requiring the window to close.
- The settings-window footer retains `OK`, `Apply`, and `Cancel`. `Apply` performs that operation and leaves the window open. `OK` performs the same operation and hides the window only after the complete durable update succeeds.
- Validation or persistence failure keeps the settings window open, preserves its staged draft, and reports the exact failure without partially accepting the update.
- `Cancel` and ordinary settings-window close discard unapplied staged edits and hide the window without changing active settings.
- Closing or hiding the settings window without applying discards unapplied staged edits and does not mutate active theme, operation, notification, developer-instructions, or other active settings.
- Ordinary settings drafts do not live-preview unapplied changes. User-visible theme Preview behavior is owned by `doc/features/theming/design.md`.

## Feature-Owned Settings Rows

- The settings feature owns row mechanics, staging, validation dispatch, apply sequencing, and persistence plumbing.
- The feature that owns a setting owns that setting's semantics.
- The Operations section includes `Context compaction timeout`; its selected-thread compaction semantics are owned by `doc/features/status-line/design.md`.
- The Operations section includes `Draft autosave interval`; its required 5-through-300-second range and 30-second default are owned by `doc/features/composer/design.md`.
- The Agent section includes `Developer Instructions`; send-time developer-instructions behavior is owned by `doc/features/composer/design.md`.
- The Notifications section includes `End-turn sound`; notification playback semantics are owned by `doc/features/notifications/design.md`.
- The Themes section and theme editor are owned by `doc/features/theming/design.md`.
- Feature-owned rows must keep controls reachable and labels readable at supported minimum settings-window width.

## Settings Dynamic Tools

- Beryl may expose bounded app-server dynamic tools for reading Beryl-owned GUI settings, validating settings updates, and committing typed settings updates.
- Settings tools operate only on Beryl-owned GUI settings. They must not expose or mutate backend-owned Codex authentication, session storage, configuration, skills, MCP state, transcript history, Syndic thread state, runtime/root records, or durable image assets.
- Settings read tools return bounded snapshots for model use.
- Literal values are returned only for non-sensitive scalar settings.
- Notification sound reads return configured/disabled state and non-identifying file metadata, never the full path.
- Developer-instructions reads return enabled state, character count, line count, and stable content fingerprint, never literal instruction text.
- Settings write tools use typed operation-specific schemas.
- Validation-only calls are non-mutating.
- Accepted settings updates commit immediately through the same validation and persistence path as Apply, without creating unapplied settings-window drafts.
- AI-control preferences that govern model authority are read-only to the model unless a later design adds an operator-confirmed write operation.
- Tool calls must cross bounded shell-owned request/response bridges. Turn workers must not hold direct access to `ShellView`, GPUI handles, settings-window internals, or repository mutation handles.
- Tool calls that cannot be correlated to the active app-server thread/turn context, fail validation, target unavailable settings, or conflict with unapplied settings-window drafts reject with bounded structured results.
