# Goals

Give users durable, validated control over Beryl's appearance theme system, including installed themes, theme editing, model-assisted theme authoring, and transcript-based theme candidates.

## Non-goals

- Owning generic settings-window shell, row, draft, apply, or persistence mechanics.
- Supporting any persisted theme schema other than the current compact TOML schema.
- Listing unsaved AI-generated theme candidates in the settings window.
- Treating transient theme preview as an installed theme or durable setting.

# Decisions

## Authority

- [Theming GUI](gui.md) is the normative supplemental composition for the Themes settings page,
  theme editor subpage, and theme-candidate code-panel actions.
- The [theme runtime system](../../systems/theme-runtime/design.md) owns theme parsing, repository
  coordination, mutation reconciliation, appearance generations, cross-window application, and
  preview arbitration.
- The [Settings feature](../settings/design.md) owns the window-wide settings draft and Apply/OK
  workflow. The [Beryl-home storage system](../../systems/beryl-home-storage/design.md) owns the
  physical home and generic durable outcome boundary.

## Appearance Theme Model

- Typography and colors used by Beryl-owned UI and transcript regions are configurable through the active appearance theme.
- Every Beryl-owned visible appearance value resolves from the active theme model or a documented derivation of an active theme property.
- Theme roles cover main-window and busy-home-window chrome, toolbar, thread-lineage regions, synthetic discussion-context transcript records, discussion-status strips, buttons, inputs, transcript shell/content, Markdown blocks and inline structures, code panels, syntax-highlight tokens, user input fragments, activity rows, status cells, thread/runtime/root selectors, scrollbars, separators, flyouts, overlays, notices, media placeholders, warning/error/info states, selection/focus/disabled states, and settings-window regions.
- The theme editor presents a rooted style-role hierarchy. For each role it shows only supported
  properties and the applicable choices among a concrete value, static-parent inheritance,
  ambient-parent inheritance, and built-in fallback.
- Unsupported role-property combinations are absent rather than appearing as editable or inherited
  values. Ambient inheritance remains visibly distinct from static hierarchy inheritance for
  embedded content whose surrounding render context changes.
- Transient interaction states may change resolved color properties for hover, pressed, active, selected, focused, disabled, warning, error, info, pending, streaming, and unavailable states, but must not change widget geometry unless a widget contract permits it.
- Reusable widgets retain ownership of their canonical local UI roles, anatomy, geometry, and
  interaction-state presentation. The appearance theme supplies supported values for those declared
  local roles; it does not invent global replacements for widget-local roles or theme nonvisual
  resource limits such as detail-row caps and virtualization overscan.
- The active theme drives both main conversation windows and app-neutral style options passed into reusable settings-window mechanics.
- A theme change becomes visible across every open Beryl window as one coherent update, and a newly
  opened window uses that same complete appearance. No window renders a partial theme or a mixture
  of old and new role values.

## Theme Documents And Repository

- Persisted themes are shareable compact TOML theme documents in the single supported theme schema.
- Installed themes are stored in a portable theme repository under the Beryl home directory so users can share themes without sharing unrelated preferences.
- The installed-theme collection may be arbitrarily large. The Themes page keeps visible work
  bounded, loads more installed rows as navigation requires them, and preserves stable selection,
  focus, and scroll position across coherent repository refreshes.
- Rename, delete, reorder, install, Save, and Save As become visible only as complete repository
  updates. Failure preserves the last coherent installed collection and staged editor state and
  reports localized feedback; an indeterminate outcome remains visibly reconciling until the exact
  old or new collection is proven.
- Delete is unavailable for the durable-active theme, the current Settings-staged active target,
  or a theme bound to an open theme-document draft or repository operation. The Delete action's
  bounded selected-theme detail row explains which reference must first be changed, committed,
  reset, discarded, or completed;
  Delete never implicitly changes active identity or discards either kind of draft.
- Successful deletion of an unreferenced theme removes only that installed row. It preserves the
  durable and staged active identities, current appearance, preview, Settings draft, and every
  unrelated theme-document draft; if the deleted row was selected, selection moves to the nearest
  surviving row or the page's empty installed-theme state.
- Validation reports unsupported roles or properties, invalid values, oversized supported values,
  and structurally invalid documents with bounded localized feedback and no visible or durable
  partial application.
- The active theme selection persists as Beryl-home settings state separately from the portable
  installed-theme repository.
- Unsupported entries in installed theme files are ignored on load and omitted on later saves.
- Installed theme documents and active theme selection are Beryl-owned state, never backend-owned
  Codex configuration.
- Each installed theme has one directly editable compact TOML file at
  `<beryl-home>/themes/installed/<stable-theme-id>.toml`. Editing that file outside Beryl is a
  supported workflow rather than corruption or an unsupported repository modification.
- A valid external edit to the active theme becomes visible automatically as one coherent update
  across all Beryl windows. A valid edit to another installed theme becomes the document used by
  its next validation, edit, preview, or activation operation.
- An invalid, partial, missing, or unreadable external edit while Beryl is running preserves the
  last coherent appearance and presents localized feedback for that installed theme. A later valid
  edit retries automatically; Beryl never applies a readable subset or flashes the built-in theme
  between live-edit attempts.
- Files not named by the installed manifest are not installed merely because they appear in the
  `installed` directory. Installation remains an explicit Beryl repository command.

## Active Theme Startup And Repository Refresh

- When no active identity has ever been saved, the built-in fallback is the clean default and no
  load error is reported.
- Startup presents a saved installed theme only after its identity, document, validation, and
  complete cross-window application succeed. If the saved identity cannot be resolved or its
  document is missing, unreadable, invalid, or cannot be applied, every window uses the complete
  built-in fallback instead of a partial theme.
- The startup fallback rule applies equally when the active installed file was edited externally
  while Beryl was not running: invalid startup content selects the complete built-in theme. This is
  distinct from a failed live edit, which preserves the last coherent running appearance.
- The Themes page identifies the unavailable saved theme or active-theme setting, explains the
  failure, and offers Retry without presenting the fallback as a successful load of that identity.
- A repository refresh preserves the last coherent installed collection and current coherent
  appearance until a complete replacement collection and any affected active theme are ready. If
  the active identity or document is missing, unreadable, invalid, or cannot be applied, all
  windows keep the previous coherent theme, or the built-in fallback when no installed theme has
  applied in this process.
- Refresh and activation failures appear in the affected installed theme's split-item preview and,
  while that item is selected, in its bounded detail area. Retry is an action on that feedback
  detail row, never a split-list item action. When the identity or item is unavailable, the page-
  level active-theme area contains the feedback and Retry action. Retry performs a fresh coherent
  repository read and never applies only the readable subset.

## Active Theme Selection And Theme-Document Drafts

- Choosing `Activate` stages that installed theme's identity in the Settings window-wide draft. It
  does not change appearance immediately and participates in the same all-or-nothing Apply/OK
  operation as every other modified setting.
- A staged active-theme choice must resolve to a valid, completely applicable installed theme before
  Settings Apply or OK becomes available. Reset, Cancel, and settings-window close treat that
  choice like any other unapplied setting.
- Reset on the modified active-theme Settings row restores only the staged active-theme identity to
  the identity in the current durable Settings snapshot. It clears only that scalar row's modified
  state and draft-local validation feedback; it never changes a theme-document draft, installed
  repository document or order, current appearance, or transient preview.
- A committed active-theme choice becomes visible atomically across every Beryl window. When the
  Settings commit is proven not committed, the prior active identity and appearance remain and the
  complete Settings draft is preserved according to the Settings feature.
- An indeterminate Settings outcome leaves the prior coherent appearance visible and the active-
  theme row reconciling. Beryl publishes the new theme only if reconciliation proves the complete
  Settings commit; proof of non-commit retains the old theme, and an unresolved outcome never
  guesses which identity is active.
- If the Settings commit succeeds but the saved theme cannot be applied afterward, the complete
  settings update remains durable and the staged Settings draft becomes clean. Beryl preserves the
  prior coherent appearance across every window and shows a persistent active-theme application
  failure notice with Retry; it does not roll back the durable theme identity or any unrelated
  setting. Retry resolves and applies the identity that is currently durable rather than assuming
  the identity from the failed application attempt. The notice remains until the current durable
  identity applies successfully.
- Theme-role property editing uses a separate feature-owned theme-document draft. Those staged
  values are not Settings values, do not contribute to the window-wide modified aggregate, and are
  never committed by Settings Apply or OK.
- Navigating between Themes and its editor preserves the theme-document draft while the Settings
  window remains open. Ordinary Settings Cancel or close discards an unsaved theme-document draft
  as well as following the Settings feature's own draft behavior; neither command mutates the
  installed repository.
- Save updates the edited installed document. Save As asks for a durable name and creates a new
  installed theme from the exact staged theme-document draft. These commands are absent when the
  theme-document draft is clean and never commit, discard, or reset the independent Settings draft.
- A committed Save As leaves the original installed document unchanged, adds the new installed
  identity and document, and keeps the editor selected and bound to the original installed theme.
  The original exact theme-document draft remains dirty, and its selected editor role remains
  unchanged. Save As does not activate the new theme, change the Settings draft, change the current
  appearance or preview, or navigate the editor to the new installed row.
- Save and Save As preserve the exact staged document and prior coherent installed collection on a
  proven failure. The Settings draft, current appearance, and preview are unchanged. An indeterminate
  repository outcome leaves only the theme-document operation reconciling and unavailable for
  duplication until the old, new, or unresolved outcome is known.
- For an indeterminate Save As, the last coherent installed collection remains visible and the
  editor stays selected and bound to the original installed theme with its exact dirty draft and
  selected role; the Settings draft, current appearance, and preview remain unchanged. Proof of
  non-commit becomes the proven-failure outcome; proof of commit performs the committed transition
  above. If neither complete outcome can be proven, the last coherent collection and original editor
  state remain presented, the affected repository scope stays unavailable with localized feedback,
  and Beryl never guesses that the new theme exists or selects it.
- While that repository operation is pending or reconciling, Settings Cancel, settings-window
  close, and Application Exit do not discard the document draft or complete. Exact old or new
  resolution reenables them; an unresolved repository outcome keeps the affected document
  unavailable with feedback but reenables close and Exit without claiming a durable outcome.
- Saving the active theme updates the durable active-theme baseline. Without a preview, its complete
  resolved appearance replaces the prior appearance atomically; while preview is active, the
  preview remains visible and Stop Preview restores the newly saved active theme. Save As never
  activates the new theme automatically.
- If the active document is saved durably but its resolved appearance cannot be applied, the
  document remains saved and its draft becomes clean, the prior coherent appearance remains
  visible, and the feature uses the same persistent application-failure notice and current-durable-
  identity Retry behavior defined above. It does not roll back the saved document.

## Themes Settings Page

- The Themes settings page is hosted inside the generic settings window defined by the
  [Settings feature](../settings/design.md).
- The Themes page lists only durable installed themes plus the active theme, with bounded loading as
  the user navigates an exhaustive installed collection.
- When the built-in fallback is the current appearance, the page presents it as a readonly active
  fallback rather than as an installed repository row; it cannot be renamed, deleted, edited, or
  saved.
- It does not list unsaved AI-generated theme candidates from Codex threads.
- Each installed-theme split-list item contains only its stable item identity, theme name label,
  optional stable-id subtext, applicable active or modified preview, and selection state. It contains
  no Copy ID, Activate, Rename, Delete, Edit, Retry, Save, or Save As command.
- Selecting an installed theme exposes a bounded selected-theme detail area. Its stable-id row shows
  the exact id and a `Copy ID` action, and the finite page-action area contains the valid `Activate`,
  `Rename`, `Delete`, and `Edit` commands for that selection.
- `Copy ID` is present whenever an installed theme is selected and copies that selected theme's
  stable id. It follows the containing Settings interaction gate rather than remaining actionable
  while the window is gated.
- A referenced selected theme's Delete action remains visible but unavailable with the reference
  explanation defined above.
- Installed non-active themes stage selection through Activate and the Settings Apply/OK workflow.
  There is no separate installed-theme Preview action.
- The selected active theme's bounded action-only detail row exposes Save and Save As when the active
  theme has staged changes.
- When the active theme has no staged changes, Save and Save As are absent rather than disabled.
- The selected active theme's Edit action opens the theme editor subpage described by the
  [Theming GUI](gui.md).
- Ordinary settings drafts do not live-preview theme changes. Preview controls are limited to unsaved `beryl-theme` transcript candidates and dynamic theme tools.

## Theme Preview Lifecycle

- Beryl owns at most one transient theme preview for the running instance. The preview applies as
  one coherent appearance across every open window, including Settings, and newly opened windows
  join that same preview.
- Transcript candidates and dynamic-tool calls initiate the same preview. Preview invocation order
  establishes precedence across both sources: a later invocation supersedes every earlier pending
  request, and only the latest request may apply. A stale validation or application completion
  cannot replace a later request.
- A successfully applied latest request replaces the current preview without first flashing the
  durable theme. If the latest request fails validation or application, the prior coherent
  appearance remains visible; an earlier superseded request does not revive.
- Stop Preview supersedes pending preview work and restores the currently durable active theme
  atomically across all windows. A successfully committed and published Settings active-theme
  change also ends preview and makes that installed theme the coherent durable appearance.
- If Stop Preview cannot validate or apply the durable active theme, the current preview remains as
  the prior coherent appearance and the invoking panel or tool reports the failure. No window
  switches partially and the durable active identity remains unchanged.
- Preview is never persisted. Successful Application Exit and process restart discard it, and
  startup applies only the durable active theme or the built-in fallback.
- A transcript-initiated preview reports validation or application failure in its originating code
  panel. A dynamic-tool-initiated preview returns a bounded structured error. Neither failure
  changes the durable active identity, installed repository, staged editor values, or prior
  coherent appearance.

## Theme Candidate Code Panels

- A fenced transcript code block with language `beryl-theme` is ordinary Codex transcript content rendered through the shared code panel widget.
- A valid `beryl-theme` panel may expose Beryl-owned `Preview` and `Install Theme` actions.
- Preview validates and submits the candidate to the instance-wide preview lifecycle without installing or persisting it.
- The originating code panel exposes Stop Preview only while its candidate owns the current preview.
- Install validates the candidate, asks for a durable theme name, and writes it into the theme repository.
- After install, choosing the new theme still stages Activate through Settings and requires Apply or
  OK; installation never activates it automatically.
- Malformed, unsupported, partial, or invalid theme code blocks render bounded panel-local validation feedback and must not mutate active theme state, settings drafts, or repository files.
- Theme candidate actions do not create synthetic transcript rows.
- The durable proposal is the original Codex transcript code block.
- Unsaved candidates remain scoped to the originating Codex thread and do not appear as a settings-window candidate inbox.

## Theme Dynamic Tools

- Beryl exposes bounded app-server dynamic tools for inspecting theme schema, reading theme
  authoring guidance, validating theme documents, starting or stopping preview, installing themes,
  updating installed themes, Save As, and staging an active-theme choice.
- Theme tools operate only on Beryl-owned theme repository and active-theme state. They must not
  expose or mutate backend-owned Codex authentication, session storage, configuration, skills, MCP
  state, Syndic thread properties or history, Beryl-home runtime/root state, durable image assets,
  or unrelated settings.
- `read_theme_schema` is the bounded structural source for role ids, supported property ids, source keywords, and built-in role metadata.
- `read_theme_authoring_guide` is explanatory guidance over the same model and must not become a second independent schema.
- `validate_theme_document` uses the same validation rules as preview, install, update, and Save As.
- Theme validation is non-mutating and must not change active preview state, installed themes, settings drafts, transcript content, or repository files.
- Dynamic-tool preview participates in the same instance-wide lifecycle and precedence as
  transcript-initiated preview; it is not transcript content, an installed theme, or a durable setting.
- Dynamic-tool install writes a durable installed theme but does not synthesize transcript theme offers.
- Dynamic-tool Save As may create a new installed theme from any valid candidate. In-place Save or
  update is available only when the tool call is editing an exact installed theme and current
  document revision; an unbound candidate cannot overwrite an installed document.
- A dynamic-tool active-theme choice may update the existing Settings window-wide draft only when
  that draft can accept the exact choice without conflict. The tool never invokes Apply or commits
  an active identity; the user-visible Settings Apply/OK workflow remains required.
- Accepted theme tool writes have the same validation and complete visible outcomes as the matching
  settings-window operation.
- Tool calls that target unknown roles, unsupported properties, invalid values, unavailable sections, stale theme ids, or unsafe draft conflicts reject with bounded structured errors and must not partially apply.
