# Goals

Give users durable, validated control over Beryl's appearance theme system, including installed themes, theme editing, model-assisted theme authoring, and transcript-based theme candidates.

## Non-goals

- Owning generic settings-window shell, row, draft, apply, or persistence mechanics.
- Maintaining multiple concurrently supported historical theme schemas.
- Listing unsaved AI-generated theme candidates in the settings window.
- Treating transient theme preview as an installed theme or durable setting.

# Decisions

## Supplemental Files

- `gui.md` is a normative supplemental GUI composition file for the Themes settings page, theme editor subpage, and theme candidate code panel actions.
- The feature entry point remains authoritative for theme model, repository, tools, and how the editor fits into the theming workflow.

## Appearance Theme Model

- Typography and colors used by Beryl-owned UI and transcript regions are configurable through the active appearance theme.
- Every Beryl-owned visible appearance value resolves from the active theme model or a documented derivation of an active theme property.
- Theme roles cover main-window and busy-home-window chrome, toolbar, thread-lineage regions, synthetic discussion-context transcript records, discussion-status strips, buttons, inputs, transcript shell/content, Markdown blocks and inline structures, code panels, syntax-highlight tokens, user input fragments, activity rows, status cells, thread/runtime/root selectors, scrollbars, separators, flyouts, overlays, notices, media placeholders, warning/error/info states, selection/focus/disabled states, and settings-window regions.
- A theme defines a rooted style-role hierarchy. Each role has a hardcoded supported property set and one value source per supported property.
- Supported value sources resolve to concrete values from inline values, the same property on the static parent chain, runtime ambient parent, or built-in fallback.
- Unsupported role-property combinations are not part of the schema and do not inherit into existence.
- Runtime ambient inheritance is distinct from static inheritance and is used for embedded content whose surrounding render context changes.
- Role property sets follow semantic category: region roles expose container properties, text roles expose foreground/background and coherent typography, single-primitive roles expose `color`, and controls, rows, menus, status, media, transcript, navigation, window, and settings roles inherit from appropriate foundation roles.
- Transient interaction states may change resolved color properties for hover, pressed, active, selected, focused, disabled, warning, error, info, pending, streaming, and unavailable states, but must not change widget geometry unless a widget contract permits it.
- The active theme drives both main conversation windows and app-neutral style options passed into reusable settings-window mechanics.

## Theme Documents And Repository

- Persisted themes use compact TOML theme documents.
- Compact TOML theme documents store style roles as `[[role]]` records with `id`, optional `static_parent`, and supported property entries whose values are either source keywords or concrete inline values.
- Installed themes are stored in a portable theme repository under the Beryl home directory so users can share themes without sharing unrelated preferences.
- The repository stores a TOML manifest for installed-theme order plus one compact TOML theme document per installed theme.
- The active theme id is a scalar Beryl setting stored in the Beryl-home Fjall settings domain; it is not duplicated in the file-based theme repository manifest.
- Persisted themes use the current TOML theme schema only.
- Unsupported entries in installed theme files are ignored on load and omitted on later saves.
- A legacy flat `theme.toml` at the Beryl home root is outside the installed theme repository. Beryl must not read, import, migrate, rewrite, or delete it.
- The active theme identity and installed theme repository are GUI-owned durable settings state, not backend-owned Codex configuration.

## Themes Settings Page

- The Themes settings page is hosted inside the generic settings window defined by `doc/features/settings/design.md`.
- The Themes page lists only durable installed themes plus the active theme.
- It does not list unsaved AI-generated theme candidates from Codex threads.
- Installed theme rows show name, stable id or copy-id action, active/modified state when applicable, and valid actions such as Activate, Rename, Delete, or Edit.
- Installed non-active themes switch by direct Activate. There is no separate installed-theme Preview action.
- The active theme row exposes Save and Save As when the active theme has staged changes.
- Save persists changes to the active installed theme.
- Save As asks for a durable name and saves staged active-theme definition as a new installed theme.
- The active theme row's Edit action opens the theme editor subpage described by `gui.md`.
- Ordinary settings drafts do not live-preview theme changes. Preview controls are limited to unsaved `beryl-theme` transcript candidates and CAS theme preview tools.

## Theme Candidate Code Panels

- A fenced transcript code block with language `beryl-theme` is ordinary Codex transcript content rendered through the shared code panel widget.
- A valid `beryl-theme` panel may expose Beryl-owned `Preview` and `Install Theme` actions.
- Preview validates and applies the candidate transiently for the running Beryl instance without installing or persisting it.
- The originating code panel can expose Stop Preview while its candidate is active.
- Install validates the candidate, asks for a durable theme name, and writes it into the theme repository.
- Activation remains an installed-theme operation after install.
- Malformed, unsupported, partial, or invalid theme code blocks render bounded panel-local validation feedback and must not mutate active theme state, settings drafts, or repository files.
- Theme candidate actions do not create synthetic transcript rows.
- The durable proposal is the original Codex transcript code block.
- Unsaved candidates remain scoped to the originating Codex thread and do not appear as a settings-window candidate inbox.

## Theme Dynamic Tools

- Beryl may expose bounded app-server dynamic tools for inspecting theme schema, reading theme authoring guidance, validating theme documents, previewing themes, installing themes, updating installed themes, Save As, and activating themes.
- Theme tools operate only on Beryl-owned theme repository and active-theme state. They must not expose or mutate backend-owned Codex authentication, session storage, configuration, skills, MCP state, Syndic conversation history, Beryl-home runtime/root/thread metadata, durable image assets, or unrelated settings.
- `read_theme_schema` is the bounded structural source for role ids, supported property ids, source keywords, and built-in role metadata.
- `read_theme_authoring_guide` is explanatory guidance over the same model and must not become a second independent schema.
- `validate_theme_document` parses and resolves compact TOML through the same validation model as preview, install, update, and Save As.
- Theme validation is non-mutating and must not change active preview state, installed themes, settings drafts, transcript content, GPUI widgets, or repository files.
- Dynamic-tool preview is transient runtime state, not transcript content, an installed theme, or a durable setting.
- Dynamic-tool install writes a durable installed theme but does not synthesize transcript theme offers.
- Accepted theme tool writes flow through the same validation, active update, cache invalidation, and persistence paths used by settings-window theme operations.
- Theme tool calls must cross bounded shell-owned request/response bridges. Turn workers must not hold direct access to `ShellView`, GPUI handles, settings-window internals, or repository mutation handles.
- Tool calls that target unknown roles, unsupported properties, invalid values, unavailable sections, stale theme ids, or unsafe draft conflicts reject with bounded structured errors and must not partially apply.
