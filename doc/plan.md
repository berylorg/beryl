# Scope

Design and implement Beryl's internal theme architecture so visual style is resolved through explicit theme roles, per-property sources, static role parents, runtime ambient parents, multiple persisted installed themes, settings-window editing, transcript `beryl-theme` candidates, and Beryl-owned CAS dynamic tools for theme and GUI settings inspection and mutation.

AI-generated theme candidates are ordinary transcript fenced code blocks with language `beryl-theme`. Beryl may enhance those code panels with Preview and Install Theme actions, but it must not create synthetic transcript-only theme offer rows as the sole durable record of a proposed theme.

Current readiness: Phase 16 is in progress and blocked on the next operator live-test metrics pass. Phase 18, Phase 19, Phase 20, Phase 21, Phase 22, Phase 23, Phase 24, Phase 25, Phase 26, Phase 27, Phase 28, Phase 29, Phase 30, Phase 31, Phase 32, Phase 33, Phase 34, Phase 35, Phase 36, Phase 37, and Phase 38 implementation and automated verification are complete. No pending final-review follow-up phases remain.

Latest resumable milestone: Phase 38 completed the reusable `gpui-settings-window` selected detail-row rendering contract by documenting and enforcing a static 32-row page detail bound, keeping selected detail rows full-rendered within that bound and directing growing collections to subpages or page-local split lists. Beryl focused settings-window and diagnostics tests pass against the patched dependency. Phase 27 established that ignored local `.cargo/config.toml` path patches and their resulting `Cargo.lock` churn are normal during local development; patch-free lockfile source hygiene is a pre-publish check, not an implementation blocker while sibling dependency changes are still local. Phase 16 remains blocked on operator live-test metrics for debug-build transcript scrolling and composer typing.

Finished baseline: Phases 1-15 and 17 are complete. They established the theme architecture, resolver, schema, runtime projection, surface migration, installed-theme repository, `beryl-theme` code-panel candidates, CAS theme/settings tools, candidate recovery, render-hot-path cleanup, transcript/cache improvements, and visible code-panel projection caching.

No hacks, migration shortcuts, or temporary compatibility adapters are approved by this plan. Any such approach requires explicit operator approval before implementation.

## Edge-case checklist

- Style property precedence: verify concrete value, static-parent inheritance, ambient-parent inheritance, and built-in fallback resolve deterministically for every supported property.
- Static inheritance integrity: verify missing parents, parent cycles, unknown role ids, unknown property ids, and incompatible property value types are rejected or recover through documented fallbacks.
- Ambient inheritance integrity: verify the same role can resolve different properties in different runtime contexts, such as inline code inside final answers, user input fragments, reasoning text, popups, and settings rows.
- Explicit unset behavior: verify the model distinguishes an intentionally inherited property from a concrete value and from an invalid or missing persisted value.
- Full visual coverage: verify every Beryl-owned background, border, text foreground, text background, font family, font size, and font weight is either theme-resolved or explicitly documented as derived from a theme property.
- Settings window layout: verify broad left-side sections, one right-pane page at a time, breadcrumb/back navigation for subpages, stable setting ids, modified indicators, row context actions, fixed right-side control widths, and no clipping at the supported minimum window size.
- Settings Theme Editor split: verify the Theme Editor follows the planned two-pane model from `localtest/settings-theme-editor-mockup.svg`, with a left role list that shows each UI role and a preview of its effective font/color treatment, and a right property editor for exactly one selected role.
- Settings Theme Editor source editing: verify role static-parent metadata is displayed without free-form editing, and per-property source selection supports concrete value, static parent, ambient parent, and fallback without collapsing source semantics into resolved concrete values.
- Settings window performance: verify Theme Editor rendering avoids obsolete flat long-row work and does not do avoidable whole-page retained-state rebuilds, render-time model scans, or unnecessary sync during scroll and window drag.
- Settings split-list bounded rendering: verify page-local split lists with growing item counts render only the visible row window plus small overscan, while preserving stable item ids, selected item state, selection events, scroll position, selected-item reveal behavior, fixed row metrics, and scrollbar semantics.
- Settings render lookup hot paths: verify color preview, picker, and row-action render paths do not perform repeated whole-model row scans per rendered row.
- Settings row virtualization: verify viewport-windowed rendering is introduced only if post-split diagnostics prove it is still needed, and preserves stable setting ids, input focus, color-picker anchoring, validation messages, row action dispatch, scroll position, and scrollbar semantics.
- Settings host synchronization: verify Beryl field edits, color drags, page navigation, theme preview/activation, external theme repository changes, and preference apply/cancel do not rebuild or resync unchanged settings-window options, active-theme projections, or retained input entities.
- Settings theme candidates: verify unsaved AI-generated theme candidates do not appear in the settings window; the bridge from a thread to durable settings is through `beryl-theme` code-panel Preview and Install Theme actions or explicit theme dynamic-tool installation.
- Transcript theme candidates: verify `beryl-theme` fenced code blocks remain ordinary transcript content while Beryl code-panel actions can validate, preview, and install their formal theme definitions.
- Preview lifecycle: verify previewing a transcript candidate is transient until explicitly installed or stopped, and verify cancellation, thread switch, app restart, and later theme activation recover to a valid durable theme.
- Render hot-path ownership: verify ordinary render, transcript scroll, composer typing, and settings-window scroll/drag do not acquire active-theme locks or traverse theme resolver state once render-ready snapshots are built.
- Dependency lockfile reproducibility: during local development, ignored `.cargo/config.toml` path patches and path-derived `Cargo.lock` churn are expected. Before publishing dependency changes, verify lockfiles are regenerated from committed manifests without ignored local patch configuration.
- Module-boundary preservation: verify file splits preserve public API shape, visibility constraints, tests, and behavior without adding compatibility adapters.
- Compact theme document preservation: verify missing theme properties remain absent unless the user explicitly edits them, and explicit `fallback` remains distinguishable from omission.
- Transcript metrics boundedness: verify diagnostics do not add whole-transcript render-frame work or distort debug-build measurements.
- Code-panel source coherence: verify displayed code, copy source, syntax projection, Preview, and Install Theme actions refer to the same source revision while async projection work is pending.
- Dynamic-tool output bounds: verify tool responses cap or summarize user-supplied and repository-supplied documents without losing status metadata needed for follow-up.
- Settings navigation reconciliation: verify page changes, split-list item reordering, list shrinkage, scroll offsets, focus, popup anchors, and diagnostics reconcile by stable identity without blank or stale views.
- Settings presentation hint coverage: verify host-exposed modified markers, font-family previews, and other app-neutral presentation hints are either rendered or removed from the public model.
- Settings selected-row rendering contract: verify full-rendered selected detail rows are statically bounded by model contract or changed to a bounded rendering strategy.

# Phase 16: Add transcript frame metrics and reassess long-turn virtualization (wip)

Add bounded transcript-frame metrics and use operator live testing to decide whether block-level transcript virtualization is needed.

Progress notes:

- Bounded transcript-frame metrics are implemented.
- Streaming `beryl-theme` code-panel flicker and Preview reentrancy panic were fixed.
- Blocked before finishing the phase: operator live-test metrics are still needed for debug-build transcript scrolling and composer typing.

# Phase 18: Fix settings-window row resize, clipping, and navigation glyphs (finished)

Fix the settings-window layout regressions found during live testing: fixed right-side controls, wrapped label stacks, minimum/default size, file-picker rows, Agent multiline rows, page action clipping, and thick triangle navigation glyphs.

Finished outcome:

- The sibling `gpui-settings-window` checkout now renders action-bearing single-line text rows as a right-aligned control column with the fixed-width text field above the action cluster.
- Beryl docs and focused tests cover the row resize and clipping contract.
- Operator live testing confirmed default/minimum settings-window layout, row spacing, Agent multiline editing, Theme Editor rows, Themes navigation, page action clipping, and color-picker popup sizing.

# Phase 19: Replace flat Theme Editor with role-list/property-editor split (finished)

Replace the obsolete flat long-row Theme Editor with the planned split shown in `localtest/settings-theme-editor-mockup.svg`: a left role list with per-role previews and a right property editor for the selected role.

Finished outcome:

- The sibling `gpui-settings-window` checkout now exposes app-neutral page-local split content with selected items, subtext, compact preview style metadata, and `PageSplitItemSelected` events.
- Beryl now renders the Theme Editor as a role-list/property-editor split. The broad settings sidebar remains on `Themes`, selected-role state is keyed by theme role id, and the right pane shows only the selected role's editable properties.
- Role previews are built from the draft theme projection so foreground, background, border, font family, font size, and font weight update as valid draft values change.
- The obsolete flat `settings/appearance` editor module was removed; the public `AppearanceSettings` conversion layer remains for compatibility and tests.
- Operator live testing confirmed the split Theme Editor now opens cleanly at default/minimum size, split detail labels no longer collapse, broad-section switching is fast, Save/Save As/Back remain reachable, and color-picker popups are bounded.

# Phase 20: Add bounded rendering for page-local split lists (finished)

Fix sluggish Theme Editor role-list scrolling by making the reusable `gpui-settings-window` page-local split list render a bounded visible window instead of all items.

Finished outcome:

- The sibling `gpui-settings-window` checkout now documents page-local split lists as bounded fixed-height selector surfaces and gives them independent tracked scroll state, managed scrollbar visibility, fixed row metrics, selected-item reveal by model index, and visible-window rendering with top/bottom spacers.
- Long split-list rendering is now bounded by viewport plus overscan. The new `gpui-settings-window` tests prove a 176-item split list reports a much smaller rendered window, preserves total scroll extent, reveals an initially offscreen selected item, keeps split-list scroll independent from detail-row scroll, and preserves `PageSplitItemSelected` event targeting.
- Beryl's existing Theme Editor regression tests prove it still supplies all `BerylThemeRole::ALL` roles, selected-role changes update only the right property editor, and role preview styling comes from the draft projection. Those focused tests passed against the patched dependency.
- Operator live testing confirmed Theme Editor role-list scrolling is now reasonably fast in the Cargo debug build.

# Phase 21: Add fully source-aware Theme Editor property controls (finished)

Implement the missing Theme Editor semantics: read-only role static-parent metadata and per-property source selection for concrete value, static parent, ambient parent, and fallback.

Finished outcome:

- The sibling `gpui-settings-window` checkout now exposes app-neutral choice rows. Choice rows carry stable string options, render as compact dropdown selectors, emit `FieldChanged` with the selected option value, and avoid retained text-input state.
- Beryl's Theme Editor draft now works over theme role definitions and property sources instead of flattening edits to concrete appearance values. It displays role static-parent metadata from schema and supports per-property source selection for concrete value, `static_parent`, `ambient_parent`, and `fallback`.
- Source and concrete value editing now share one app-neutral compound settings row. Concrete editors are present only while the source is `value`.
- The Theme Editor no longer exposes an editable static-parent text row. Role-list entries continue to show static parent metadata from Beryl's schema or existing explicit theme documents, and Save/Save As preserve explicit document static parents through the shared resolver path.
- Split-detail compound rows now use a top-aligned label/control layout, choice dropdown popups anchor from recorded selector bounds, and Beryl omits noisy per-property effective-value subtitles.
- Operator accepted the repaired Theme Editor behavior as sufficient to continue. Phase 30 now owns the remaining compact-document preservation defect found in final review.

# Phase 22: Add settings-window performance diagnostics (finished)

Add measurement after the Theme Editor split so later performance work is grounded in the corrected editor's frame and model-sync evidence rather than in the obsolete flat editor.

Finished outcome:

- The sibling `gpui-settings-window` checkout now exposes app-neutral content-free settings-window diagnostics through `SettingsWindowHandle::diagnostics_snapshot`, `SettingsWindowView::diagnostics_snapshot`, and `SettingsPanel::diagnostics_snapshot`.
- The diagnostics snapshot reports selected section/page ids, selected detail-row total/rendered counts, page-local split-list total/rendered counts and visible range, render-tree construction timing, model-sync timing, option-sync timing, input-sync counts, color-preview lookup counts, color-model lookup counts, and a dominant timing category. It does not expose setting labels, setting values, paths, validation text, developer-instructions text, or theme documents.
- Beryl now exposes the snapshot through `read_settings_window_diagnostics` in the Beryl dynamic-tool namespace and through `beryl_diagnostic.read_settings_window` for diagnostic children.
- Diagnostics distinguish bounded split-list rendering from full selected-page row rendering and distinguish model sync, option sync, input sync, color preview lookup, and color model lookup counters.

# Phase 23: Remove settings-window render-time model lookup hot spots (finished)

Eliminate repeated whole-model scans from row render paths, starting with color-preview rendering, while preserving the current non-virtualized row tree.

Finished outcome:

- The sibling `gpui-settings-window` checkout now resolves compact color swatches and active picker preview color from the currently rendered row or secondary detail field presentation value instead of performing render-time lookup through `SettingsWindowModel`.
- Invalid color drafts continue to show the latest known valid color when available, and active picker previews continue to override the persisted field value while the picker is open.
- Diagnostics now prove a render can increment the cheap color-preview counter for rendered color fields without incrementing the model-lookup counter. The retained model-lookup path remains for synchronization and test support outside row render.

# Phase 24: Reassess and, if needed, virtualize selected settings page rows (finished)

Use post-split diagnostics to decide whether generic selected-page row virtualization is still needed. If needed, implement viewport-windowed row rendering; otherwise record that virtualization is unnecessary for this plan.

Finished outcome:

- Post-split diagnostics and code inspection show `gpui-settings-window` still full-renders selected page detail rows by design, with diagnostics reporting `row_height_strategy = "full_selected_page"`, `rendered_row_count = total_row_count`, and no visible range.
- Beryl's split Theme Editor no longer creates an unbounded selected detail-row surface. The selected role property editor contains one Save As row plus the seven schema-owned `BerylThemeProperty::ALL` property rows; the large 176-item role selector is the page-local split list already virtualized in Phase 20.
- Generic selected-page row virtualization is therefore unnecessary for this Beryl plan phase. The remaining settings-window drag/scroll repaint profile should be addressed by the later scrollbar invalidation and host synchronization phases rather than by variable-height selected-row virtualization.
- Phase 38 now owns the final-review request to document or enforce the reusable crate's selected detail-row rendering bound.

# Phase 25: Gate settings-window scrollbar activity invalidation (finished)

Reduce redundant settings-window repaints caused by pointer movement over scroll regions after the split editor and diagnostics show the remaining repaint profile.

Finished outcome:

- The sibling `gpui-settings-window` checkout no longer forces an unconditional `cx.notify()` from `note_content_scrollbar_activity`, `note_navigation_scrollbar_activity`, or `note_split_scrollbar_activity`.
- Settings-window scroll regions still report pointer and wheel activity into `gpui-scrollbar` managed visibility state. Owner repaint requests now come from the managed scrollbar update callback only when reveal or fade phase changes need a repaint, while direct scrollbar interactions remain owned by `gpui-scrollbar`.
- A focused source regression test now proves viewport activity methods continue to call `record_viewport_activity` but do not force a panel notify for every activity event, and that the managed scrollbar visibility callback still notifies the owner.

# Phase 26: Narrow Beryl settings-window model and options synchronization (finished)

Remove host-side synchronization churn from Beryl settings edits and theme updates without changing the reusable crate's app-neutral boundary.

Finished outcome:

- Implementation started from the host synchronization contract: model updates may continue for ordinary settings edits, but options sync must be gated behind explicit changes to visual theme, saved swatches, title, undo limits, or window geometry.
- Beryl no longer calls settings-window option synchronization from `sync_settings_window_model`. Ordinary settings navigation, field edits, validation updates, and preference dynamic-tool model refreshes now update only the settings-window model unless a separate explicit options path sees changed options.
- `SettingsState` now caches the derived `SettingsWindowOptions` behind the active theme projection style revision and records the last successfully synced options snapshot, so saved color swatches and the app-neutral `SettingsWindowTheme` are rebuilt and published only when the active visual theme semantics change.
- Theme draft modified state is now maintained when theme fields or active baselines change, instead of rebuilding the full candidate theme definition every time the settings model asks whether the active theme row is modified.
- Active-theme preview, Stop Preview, Save, Save As, activation, and repository refresh paths keep explicit settings-window options synchronization. Unchanged option snapshots are skipped after the first successful sync for a given state.

# Phase 27: Define local Cargo patch and lockfile hygiene (finished)

Define the development policy for local sibling-checkout patching and the later pre-publish lockfile hygiene check.

Implementation areas:

- Treat ignored `.cargo/config.toml` sibling checkout patches as normal environment-only development state.
- Allow local path patches to rewrite `Cargo.lock` during active development.
- Before publishing dependency changes, regenerate Beryl and `gpui-settings-window` lockfiles without ignored `.cargo/config.toml` path patches affecting source entries.
- Preserve workspace dependency declarations with `workspace = true` in workspace members.

Verification for this phase:

- Local implementation verification may use ignored `.cargo/config.toml` path patches and path-derived lockfile updates.
- Before publishing dependency changes, prove `gpui`, `gpui-settings-window`, `gpui-text-input`, and `gpui-scrollbar` lockfile entries match the committed manifests' git dependency sources in a patch-free environment.
- Before publishing dependency changes, verify no committed lockfile contains local-path-derived missing source entries or unexpected `[[patch.unused]]` records from ignored local configuration.

Progress notes:

- Operator approved ignored local `.cargo/config.toml` path patching for local builds and clarified that path-derived `Cargo.lock` churn is expected during development.
- Local patched `cargo check --all-targets` in the sibling `gpui-settings-window` checkout passed.
- Local patched `cargo check -p beryl-app --all-targets` passed.
- Pre-publish lockfile cleanup remains a future hygiene step when publishing dependency changes is in scope.

# Phase 28: Split root shell module into focused internal modules (finished)

Reduce `crates/beryl-app/src/shell.rs` by moving cohesive shell-local theme, dynamic-tool, durable-task, diagnostic, and helper blocks into focused internal modules while preserving shell behavior and API shape.

Implementation areas:

- Move shell render-theme cache and style snapshot code behind a shell-owned module boundary.
- Move dynamic theme durable task state and durable theme worker code out of the root shell file.
- Move theme dynamic-tool handling and settings dynamic-tool handling into focused shell submodules that remain orchestrated by `ShellView`.
- Preserve existing visibility constraints and avoid broad behavior refactors during the split.

Verification for this phase:

- Add or update source checks proving `shell.rs` no longer carries the newly splittable final-review blocks and is materially reduced.
- Run `cargo fmt --all -- --check`, `cargo check -p beryl-app --all-targets`, focused dynamic-tool/settings/theme tests, and `git diff --check`.

Finished outcome:

- `crates/beryl-app/src/shell.rs` now wires focused shell-owned modules for render-theme snapshots, dynamic theme handling, durable theme workers, settings dynamic-tool handling, and diagnostics.
- `shell.rs` was reduced from 16,082 lines to 14,521 lines while preserving the existing shell behavior and render-facing API shape.
- Source checks now prove the moved final-review blocks stay out of the root shell file and live in their focused module homes.
- Verification passed with local Cargo path patches: `cargo fmt --all -- --check`, `cargo check -p beryl-app --all-targets`, focused source/theme/settings dynamic-tool tests, diagnostic dynamic-tool tests, and `git diff --check`.

# Phase 29: Split newly introduced oversized theme and settings modules (finished)

Bring new large theme/settings files under focused module boundaries where the split is low-risk and improves ownership clarity.

Implementation areas:

- Split Theme Editor draft/model construction, field id helpers, and property row construction out of the single `theme_editor.rs` file where practical.
- Split theme dynamic-tool parsing, response construction, schema/guide output, and mutation helpers into focused modules.
- Split settings dynamic-tool parsing and response helpers if they remain above the rough file-size threshold after Phase 28.
- Split theme repository storage helpers where persistence, document IO, and recovery logic can be separated without changing repository behavior.

Verification for this phase:

- Verify no newly introduced file remains substantially above the rough split threshold unless a local documented reason proves further split is not reasonable.
- Run focused theme editor, theme repository, theme dynamic-tool, settings dynamic-tool, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

Finished outcome:

- `shell/render_theme.rs` now delegates shell-local button theme types, render-frame delegation, and role-style resolution to focused internal modules.
- `shell/settings/theme_editor.rs` now remains the Theme Editor facade while field ids, draft definition construction, property rows, and schema/value helpers live in focused submodules.
- `theme_dynamic_tools.rs` and `settings_dynamic_tools.rs` now remain dynamic-tool API/spec facades while parsing and response construction live in focused submodules.
- `appearance/theme/repository/store.rs` now keeps the public repository API while manifest/document IO and snapshot projection live in focused internal modules.
- A Phase 29 source check proves all split target files stay below the rough 500-line threshold and that moved blocks remain in their module homes.
- Verification passed with local Cargo path patches: `cargo fmt --all -- --check`, `cargo check -p beryl-app --all-targets`, focused Theme Editor/theme repository/theme dynamic-tool/settings dynamic-tool/source tests, transcript theme source tests, and `git diff --check`.

# Phase 30: Preserve compact theme documents through editor saves (finished)

Fix Theme Editor save behavior so compact installed theme documents remain compact and source-faithful after editing one property.

Implementation areas:

- Represent omitted role properties distinctly from explicit `fallback`, explicit inherited sources, and concrete values in the editor draft.
- When saving an existing compact theme, preserve omitted properties and omitted roles unless the user explicitly edits them.
- Preserve explicit static parents and explicit property sources through preview, Save, Save As, validation, and repository persistence.

Verification for this phase:

- Add tests loading partial repository theme documents, editing one property, saving, and proving unrelated omitted properties remain absent in the serialized document.
- Add tests distinguishing omitted property, explicit `fallback`, `static_parent`, `ambient_parent`, and concrete value after editor round-trip.
- Run focused Theme Editor, theme document format, theme repository, theme resolver tests, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

Finished outcome:

- Theme Editor draft generation now preserves omitted role properties and omitted roles unless the corresponding source or concrete value field is explicitly edited.
- Existing compact documents still display omitted properties through the current fallback source UI choice, but saving an unrelated field no longer materializes those omissions as explicit `fallback`.
- Explicit `fallback`, `static_parent`, `ambient_parent`, and concrete values survive Save and Save As, and selecting fallback on an omitted property records explicit fallback as user intent.
- Regression tests now cover compact repository documents through Theme Editor Save and Save As, direct compact document serialization, theme repository persistence, and resolver behavior.
- Verification passed with local Cargo path patches: focused Theme Editor/theme document/theme repository/theme resolver tests, `cargo fmt --all -- --check`, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

# Phase 31: Remove render-path active-theme lock and resolver work (finished)

Ensure ordinary render, transcript scroll, and composer typing consume render-ready snapshots without locking active theme state or traversing the theme resolver.

Implementation areas:

- Build and publish render-ready style snapshots when active theme projection changes, not lazily from render snapshot calls.
- Replace render-frame cache misses that lock `active_theme` with shell-owned immutable snapshots or explicitly synchronized revisions.
- Keep fallback behavior for poisoned or unavailable theme state off hot render paths.

Verification for this phase:

- Add source checks or tests proving `render_style_snapshot`, transcript panel snapshot construction, and composer render paths do not lock `active_theme` or call theme resolver APIs.
- Verify theme preview, Stop Preview, activation, Save, Save As, and repository refresh still update visible style exactly once per accepted change.
- Run focused theme source/render tests, settings/theme tests, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

Finished outcome:

- Shell render-theme state now stores an eagerly built `ShellRenderStyleSnapshot`; `render_style_snapshot` clones that published snapshot and no longer locks `active_theme`, rebuilds the render cache, or calls resolver-backed theme construction.
- Accepted active-theme projection changes now flow through shell-owned publication before surface notification for transcript theme candidates, dynamic-tool preview and Stop Preview, durable dynamic theme repository updates, settings-window activation, Save, and Save As.
- Dynamic theme preview and durable repository apply paths share the existing single refresh/options-sync path instead of layering extra shell notifications.
- Source checks now prove `render_style_snapshot`, transcript panel snapshot construction, and composer render paths avoid `active_theme` locking and resolver APIs while accepted theme mutations publish the render snapshot before notifying visible surfaces.
- Verification passed with local Cargo path patches: focused theme source/render tests, settings/theme tests, `cargo fmt --all -- --check`, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

# Phase 32: Keep transcript frame metrics bounded (finished)

Fix debug transcript frame metrics so measurement collection does not scan all retained transcript rows during render-frame snapshotting.

Implementation areas:

- Replace whole-presentation row scans in render metrics with incrementally maintained counts or visible-window-bounded measurements.
- Keep transcript metrics content-free and bounded by configured diagnostic limits.
- Preserve Phase 16 live-test usefulness by ensuring metrics overhead is not the dominant debug-build scroll cost.

Verification for this phase:

- Add tests proving metrics collection work is bounded by visible or retained diagnostic limits rather than total transcript row count.
- Verify transcript frame metric output still reports the counts needed for the Phase 16 long-turn virtualization decision.
- Run focused transcript metrics/diagnostic tests, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

Finished outcome:

- `TranscriptPresentationState` now caches aggregate frame metric counts when presentation rows are created, prepended, appended, replaced, removed, or converted to released-history placeholders.
- DEBUG transcript panel snapshots still include total loaded turn count, total projected item count, total projected text size, visible range, inspected panel rows, and timing fields, but `render_metrics()` is now an O(1) read over cached presentation totals.
- Transcript presentation range helpers and row metric/identity helpers now live in focused child modules, leaving `transcript_presentation.rs` under the rough file-size threshold while preserving the existing public functions.
- Regression tests cover cached metric correctness across replace, prepend, live append, steering, assistant output, and released placeholder replacement. Source checks prove `render_metrics()` does not scan retained rows or inspect turn text.
- Verification passed with local Cargo path patches: focused transcript presentation/diagnostic/source tests, `cargo fmt --all -- --check`, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

# Phase 33: Keep code-panel display and actions source-coherent during async projection (finished)

Fix code-panel projection reuse so visible code, copy source, syntax projection, Preview, and Install Theme actions cannot refer to different source revisions while async display projection work is pending.

Implementation areas:

- Tie displayed projection identity to the same source fingerprint used by header actions and syntax controls.
- Decide whether stale display should be hidden, marked pending, or paired with matching stale actions while a newer projection is in flight.
- Preserve large-code-panel responsiveness and bounded projection-cache retention.

Verification for this phase:

- Add tests where a large streaming fenced block changes while projection work is in flight, proving displayed text and all actions target the same source revision.
- Include a `beryl-theme` fenced block case proving Preview and Install Theme cannot act on newer invisible text or older stale text.
- Run focused code-panel projection/cache, transcript theme candidate, transcript markdown render tests, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

Finished outcome:

- Code-panel projection lookup now returns a ready projection together with the displayed source revision that produced it, including display source, header/action copy source, syntax label, and fenced-copy delimiters.
- Transcript code-panel rendering now builds header actions, Copy, syntax highlighting, Preview, Install Theme, displayed text, and code-panel selection from that displayed source revision. When a newer large source is pending, the previous display remains visible only with matching previous actions; when no display projection is ready, source-targeting actions are withheld.
- The projection-cache data-shape structs now live in a focused child module so the root cache module remains under the rough file-size split threshold after the coherence change.
- Regression tests cover stale large updates, stale streaming completions, and a `beryl-theme` panel whose Preview/Install source metadata remains tied to the visible source revision until the latest projection completes.
- Verification passed with local Cargo path patches: focused code-panel projection/cache, syntax, transcript markdown render, transcript theme candidate/source tests, `cargo fmt --all -- --check`, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

# Phase 34: Bound active theme document dynamic-tool output (finished)

Make `read_theme_repository(includeActiveDocument=true)` bounded while preserving enough metadata for follow-up inspection and mutation.

Implementation areas:

- Add byte or structural limits to active theme document serialization in theme dynamic-tool responses.
- Report truncation metadata when the active document exceeds the response cap.
- Validate or cap repository-loaded string property values where needed so dynamic-tool output cannot be driven unbounded by theme document contents.

Verification for this phase:

- Add tests with oversized font-family or document content proving `includeActiveDocument=true` returns bounded output and explicit truncation metadata.
- Verify ordinary compact active documents still return complete text when under the cap.
- Run theme dynamic-tool, theme document format, theme repository tests, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

Finished outcome:

- `read_theme_repository(includeActiveDocument=true)` now returns `activeDocument` as a structured object containing the active theme id, name, bounded TOML text, original byte length, retained byte length, omitted byte length, byte limit, and `truncated` status. Metadata-only repository reads still return `activeDocument: null`.
- Active document text is capped at the dynamic-tool response limit on a UTF-8 boundary. Compact active documents below the cap remain complete, while oversized generated documents return explicit truncation metadata.
- Theme font-family values are capped during theme validation, appearance settings validation, and repository document loading, so oversized persisted theme documents are rejected instead of inflating active-document serialization.
- Regression tests cover bounded oversized active-document output, complete under-limit active documents, oversized font-family document rejection, repository recovery that skips oversized persisted theme documents, and appearance settings validation for the same cap.
- Verification passed with local Cargo path patches: focused theme dynamic-tool/theme document/theme repository/appearance settings tests, `cargo fmt --all -- --check`, `cargo check -p beryl-app --all-targets`, and `git diff --check`.

# Phase 35: Reconcile settings selected-page scroll and focus on same-section navigation (finished)

Fix `gpui-settings-window` so switching pages inside the same broad section does not inherit stale detail scroll offsets or focus from the previous page.

Implementation areas:

- Decide and implement the app-neutral policy for per-page detail scroll retention versus reset-on-page-change.
- Reconcile focused inputs when the selected page changes and the focused field is no longer present.
- Preserve breadcrumb/back navigation, section selection, color picker closure or anchoring policy, and selected field focus behavior.

Verification for this phase:

- Add `gpui-settings-window` tests for same-section page changes after nonzero scroll and focused input state.
- Add Beryl settings-window tests if Beryl relies on same-section theme subpage navigation behavior.
- Run `cargo nextest run --test panel_window --test presentation`, `cargo check --all-targets` in `gpui-settings-window`, focused Beryl settings-window tests if dependency output changes, and `git diff --check`.

Finished outcome:

- The sibling `gpui-settings-window` checkout now treats a selected-page id change as a page-local state boundary: right-pane detail scroll resets to the top, content scrollbar visibility resets, transient choice/color-picker popups close, and focus moves to the first text-capable field on the new selected page or to the panel when none exists.
- Same-page model refreshes preserve the selected detail scroll offset and focused retained input when the referenced controls still exist.
- The reusable crate design documents the selected-page navigation policy without adding Beryl-specific behavior.
- Regression tests cover same-section page changes after nonzero scroll and focused input state, same-page model refresh preservation, and color-picker popup closure on same-section page change. The existing Beryl settings-window focused tests pass against the patched dependency.
- Verification passed with local Cargo path patches: `cargo nextest run --test panel_window --test presentation --test color_picker`, `cargo check --all-targets` in `gpui-settings-window`, `cargo nextest run -p beryl-app --test appearance_settings_window`, `cargo check -p beryl-app --all-targets`, `cargo fmt --all -- --check`, and `git diff --check`.

# Phase 36: Reconcile page-local split-list scroll after model refresh (finished)

Fix page-local split lists so item reordering, selected-item index changes, and list shrinkage reconcile scroll offset and diagnostics even when the selected item id is unchanged.

Implementation areas:

- Recompute selected item reveal and scroll clamping from current item count and selected index on model refresh.
- Prevent stale offsets from producing `start > end`, blank rendered windows, or misleading diagnostics.
- Preserve stable item ids, selected item state, selection events, independent detail-pane scroll state, and scrollbar semantics.

Verification for this phase:

- Add `gpui-settings-window` tests for selected item moving index with the same id, list shrinkage below current scroll offset, and diagnostics rendered range consistency.
- Live-test or screenshot-check Theme Editor role-list refresh behavior if Beryl can change role-list presentation while open.
- Run `cargo nextest run --test panel_window --test presentation`, `cargo check --all-targets` in `gpui-settings-window`, focused Beryl Theme Editor tests, and `git diff --check`.

Finished outcome:

- The sibling `gpui-settings-window` checkout now compares the refreshed split-list selection by page id, stable item id, and current selected index, so a same-id selected item that moves to a new offscreen index is revealed after model sync.
- Same-page split-list refreshes preserve valid split scroll positions when the selected item index is unchanged, while clamping stale offsets to the refreshed list extent when the item count shrinks.
- Split-list render-window calculation clamps incoming offsets to the current item extent before computing the bounded visible range, preventing `start > end`, blank non-empty split windows, and misleading diagnostics.
- The reusable crate design documents the app-neutral split-list refresh reconciliation policy without adding host-specific behavior.
- Regression tests cover same-id selected-item reordering, list shrinkage below the current scroll offset, and diagnostics rendered-range consistency. Beryl Theme Editor focused tests pass against the patched dependency.
- Verification passed with local Cargo path patches: `cargo nextest run --test panel_window --test presentation`, `cargo check --all-targets` in `gpui-settings-window`, `cargo nextest run -p beryl-app --test appearance_settings_window`, `cargo check -p beryl-app --all-targets`, `cargo fmt --all -- --check`, and `git diff --check`.

# Phase 37: Render or remove app-neutral settings presentation hints (finished)

Close the gap between `gpui-settings-window` presentation model APIs and rendered output for secondary modified state and split-item font-family previews.

Implementation areas:

- Render secondary detail-field modified state in a way consistent with primary row modified indicators, or remove the app-neutral API if hosts must not set it.
- Apply split-item font-family preview hints to the preview label/sample, or remove the hint from the public model and docs.
- Keep the behavior app-neutral and avoid Beryl-specific theme-role knowledge in the reusable crate.

Verification for this phase:

- Add presentation tests proving secondary modified state is visible or no longer exposed.
- Add presentation tests proving split-item font-family preview hints are rendered or no longer exposed/documented.
- Run `cargo nextest run --test presentation --test panel_window`, `cargo check --all-targets` in `gpui-settings-window`, focused Beryl Theme Editor preview tests if output changes, and `git diff --check`.

Finished outcome:

- The sibling `gpui-settings-window` checkout now renders secondary detail-field modified state with the same app-neutral modified indicator used by primary rows and pages.
- Page-local split-list item preview font-family hints now apply to the primary preview label while preserving ordinary supporting subtext styling.
- The public presentation APIs remain intact because both hints are part of the reusable crate's documented app-neutral model, and no Beryl-specific theme-role knowledge was added to the crate.
- Regression tests cover rendered secondary modified state and rendered split-item font-family preview hints.
- Verification passed with local Cargo path patches: `cargo nextest run --test presentation --test panel_window`, `cargo check --all-targets` in `gpui-settings-window`, `cargo nextest run -p beryl-app --test appearance_settings_window`, `cargo check -p beryl-app --all-targets`, `cargo fmt --all -- --check`, and `git diff --check`.

# Phase 38: Document or bound selected settings detail-row rendering (finished)

Resolve the selected detail-row rendering contract in `gpui-settings-window` so full rendering is justified by a durable static bound or replaced with a bounded strategy.

Implementation areas:

- Determine whether selected detail pages are statically bounded by the app-neutral model contract.
- If full rendering remains valid, document and enforce the maximum selected detail-row count or the construction rule that proves it small.
- If no durable bound exists, implement a bounded rendering strategy that preserves focus, inputs, color picker anchoring, row actions, validation text, modified indicators, scroll position, and scrollbar semantics.

Verification for this phase:

- Add tests proving either enforced static row bounds or viewport-windowed selected detail-row rendering.
- Verify Theme Editor selected-role detail rows remain correct and responsive at the supported minimum settings-window size.
- Run `cargo nextest run --test panel_window --test presentation --test color_picker`, `cargo check --all-targets` in `gpui-settings-window`, focused Beryl settings-window tests, and `git diff --check`.

Finished outcome:

- The sibling `gpui-settings-window` checkout now documents selected page detail rows as a static full-rendered surface bounded to 32 rows per page; hosts with growing collections must use subpages or page-local split lists.
- The reusable crate exports `MAX_PAGE_DETAIL_ROWS` and model validation rejects oversized pages with `SettingsWindowError::TooManyPageRows`, so the existing `full_selected_page` diagnostics strategy is backed by a durable model contract.
- Regression tests cover rejecting pages over the static detail-row bound and diagnostics for a maximum-sized selected page that full-renders exactly the bounded row count with no visible range or overscan.
- Beryl focused settings-window and diagnostics tests pass against the patched dependency, proving the Theme Editor selected-role detail rows and current Beryl settings pages remain within the reusable crate bound.
- Verification passed with local Cargo path patches: `cargo nextest run --test panel_window --test presentation --test color_picker`, `cargo check --all-targets` in `gpui-settings-window`, `cargo nextest run -p beryl-app --test appearance_settings_window --test diagnostic_dynamic_tools`, `cargo check -p beryl-app --all-targets`, `cargo fmt --all -- --check`, and `git diff --check`.
