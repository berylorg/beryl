# Scope

Design and implement Beryl's internal theme architecture so visual style is resolved through explicit theme roles, capability-specific per-role properties, per-property sources, static role parents, runtime ambient parents, multiple persisted installed themes, settings-window editing, transcript `beryl-theme` candidates, and Beryl-owned CAS dynamic tools for theme and GUI settings inspection and mutation.

AI-generated theme candidates are ordinary transcript fenced code blocks with language `beryl-theme`. Beryl may enhance those code panels with Preview and Install Theme actions, but it must not create synthetic transcript-only theme offer rows as the sole durable record of a proposed theme.

Current readiness: Phase 16 is in progress and blocked on operator live-test metrics for debug-build transcript scrolling and composer typing. The theme role-property capability, validation, separator `color`, role narrowing, and Theme Editor/tool presentation improvements are complete.

Latest resumable milestone: Phase 43 updated Theme Editor presentation and model-facing theme tool output so supported role-property inventories are presented explicitly and unsupported properties are not advertised or reintroduced by editor saves.

Completed baseline: Phases 1-15 and 17-43 are complete. They established the theme architecture, resolver, schema, runtime projection, surface migration, installed-theme repository, `beryl-theme` code-panel candidates, CAS theme/settings tools, candidate recovery, render-hot-path cleanup, transcript/cache improvements, visible code-panel projection caching, settings-window performance fixes, selected-detail row bounds, theme-property capability plumbing, strict-validation/tolerant-load policy for unsupported theme properties, separator `color` semantics, role-specific property narrowing, and capability-specific editor/tool presentation. Finished implementation phases are intentionally summarized here rather than retained as inline phase sections.

No hacks, migration shortcuts, or temporary compatibility adapters are approved by this plan. Any such approach requires explicit operator approval before implementation.

## Edge-case checklist

- Style property precedence: verify concrete value, static-parent inheritance, ambient-parent inheritance, and built-in fallback resolve deterministically for every supported property.
- Static inheritance integrity: verify missing parents, parent cycles, unknown role ids, unknown property ids, unsupported role-property combinations, and incompatible property value types are rejected or recover through documented fallbacks according to the caller path.
- Theme property capabilities: verify each role exposes only properties consumed or derived by its render sites, unsupported role-property combinations are absent from schema/editor output, and unsupported combinations are rejected by candidate validation.
- Ambient inheritance integrity: verify the same role can resolve supported properties differently in different runtime contexts, such as inline code inside final answers, user input fragments, reasoning text, popups, and settings rows.
- Explicit unset behavior: verify the model distinguishes an intentionally inherited property from a concrete value and from an invalid or missing persisted value.
- Full visual coverage: verify every Beryl-owned background, border, single-primitive color, text foreground, text background, font family, font size, and font weight is either theme-resolved or explicitly documented as derived from a theme property.
- Separator color semantics: verify separator roles expose and render from `color`, old persisted separator `border` entries are ignored on installed-theme load, and `border` does not migrate into or override `color`.
- Installed-theme recovery: verify unsupported persisted properties are ignored on repository load without dropping the installed theme when supported properties can resolve through fallback or inheritance.
- Strict candidate validation: verify `validate_theme_document`, transcript candidate Preview/Install, installed-theme update, and Save As reject unsupported role-property combinations before mutating preview, repository, settings draft, or transcript state.
- Compact theme document preservation: verify missing supported theme properties remain absent unless the user edits them, unsupported ignored persisted properties are not reserialized, and explicit `fallback`, `static_parent`, `ambient_parent`, and concrete values continue to round-trip for supported properties.
- Settings Theme Editor split: verify the Theme Editor keeps the two-pane model, stable selected-role state, bounded detail rows, source editing, and role previews after selected-role properties become capability-specific.
- Dynamic-tool output bounds: verify schema, guidance, validation, and repository tool responses remain bounded while accurately reporting supported property inventories.
- Render hot-path ownership: verify ordinary render, transcript scroll, composer typing, and settings-window scroll/drag do not acquire active-theme locks or traverse resolver state once render-ready snapshots are built.
- Module-boundary preservation: verify file splits preserve public API shape, visibility constraints, tests, and behavior without adding compatibility adapters.
- Dependency lockfile reproducibility: during local development, ignored `.cargo/config.toml` path patches and path-derived `Cargo.lock` churn are expected. Before publishing dependency changes, verify lockfiles are regenerated from committed manifests without ignored local patch configuration.

# Phase 16: Add transcript frame metrics and reassess long-turn virtualization (wip)

Add bounded transcript-frame metrics and use operator live testing to decide whether block-level transcript virtualization is needed.

Progress notes:

- Bounded transcript-frame metrics are implemented.
- Streaming `beryl-theme` code-panel flicker and Preview reentrancy panic were fixed.
- Blocked before finishing the phase: operator live-test metrics are still needed for debug-build transcript scrolling and composer typing.
