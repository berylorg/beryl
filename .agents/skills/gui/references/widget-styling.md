# Widget Styling Authoring

Read this reference together with the GUI skill when creating, editing, reviewing, or implementing UI roles or widget `Spec CSS`. The rules in this reference are normative.

## UI Role Addressing

Every widget spec has one widget role name.

For project-local specs, use `<widget-name>` from `doc/gui/widgets/<widget-name>/spec.md`.

For built-in specs, use the final directory name under `references/widget-specs/`.

For externally registered specs, use a canonical widget name listed in `doc/gui/external-specs.md`.

If a widget spec explicitly declares `Role name:` in `# Name`, that value overrides the path-derived role name.

Define UI role defaults inside `# UI Roles` as CSS custom-property declarations, not Markdown lists. Use one fenced `css` block unless the section is `N/A`.

Selectors in `# UI Roles` follow the same anatomy selector rules as `Spec CSS`.

Declarations in `# UI Roles` must be CSS custom properties only. Use local property names such as `--height`, `--padding-x`, `--background`, and `--ring-width`. Do not repeat the widget role name in the custom property name. Do not encode part or state names in the custom property name when the selector already names that part or state.

Expand each custom property declaration into a canonical role id with:

```text
<widget-role-name>[.<part>][.<state>].<property>
```

Use these selector rules:

- The root selector, such as `.command-button`, adds no part or state prefix.
- A part selector, such as `.command-button__icon`, adds the part name.
- A root state selector, such as `.command-button[data-state~="hover"]`, adds the state name.
- A part state selector, such as `.context-menu__row[data-state~="hover"]`, adds the part name and then the state name.

For a `context-menu` widget, `.context-menu__row[data-state~="hover"] { --background: #eef2f7; }` expands to `context-menu.row.hover.background`.

Theme-aware apps use the expanded canonical role ids directly or through a deterministic adapter for their theme system. Apps without theming use the fallback values listed in the widget spec.

The default visual variant belongs in `# Variants`. Exact default visual and layout fallback values belong only in `# UI Roles`.

Prefer `foreground` for text, icon, and stroke color; `background` for fills; `width` and `height` for rectangular dimensions; `size` only when one value intentionally controls both width and height; and `padding-x` and `padding-y` instead of ambiguous padding when axes may differ.

Every visual-impacting or layout-impacting parameter used by the default variant must have a UI role fallback unless the value is inherited from platform behavior, a documented environment value, or a documented dynamic widget-state value.

Outside `# UI Roles`, exact values are allowed only for formal identifiers, dependency names, paths, section names, state names, variant names, behavioral constants, formulas, and implementation references that do not act as default visual or layout fallback values.

## Widget CSS Notation

Use fenced `css` blocks as specification notation when CSS makes widget look or layout easier to read. CSS in widget specs describes intended visual output; it does not require a browser, DOM, browser cascade, or a CSS-capable implementation.

Prose remains authoritative for behavior. Do not use CSS to define activation, keyboard movement, focus routing, selection semantics, open/close policy, dismissal, data ownership, validation, persistence, or feature-specific workflow.

When a `Spec CSS:` block is present, keep `# Look` and `# Layout` prose to semantic intent, constraints CSS cannot express, and short orientation for the CSS contract. Do not duplicate CSS declarations, sizing formulas, spacing values, state colors, overflow rules, or placement formulas in prose unless the duplication is needed to disambiguate a non-CSS semantic rule.

`Spec CSS:` blocks must reference UI role custom properties, inherited platform values, documented environment values, or documented dynamic widget-state values for default visual and layout fallback values. Do not introduce literal colors, dimensions, spacing, radii, opacity values, durations, or similar fallback values in `Spec CSS:`.

Structural CSS literals and keywords such as `display`, `position`, `box-sizing`, `flex-direction`, `align-items`, `justify-content`, `0`, `100%`, `auto`, and overflow or wrapping keywords are allowed only when they express layout mechanics such as fill, origin, reset, intrinsic sizing, alignment, clipping, wrapping, or a dynamic formula rather than a tunable widget default.

Place at most one `Spec CSS:` block at the end of `# Layout` when a widget uses CSS notation. The block may include visual state selectors because it is a compact style contract for the whole widget.

Selectors must map to widget anatomy:

- Use one root class matching the widget role name, such as `.command-button`.
- Use part classes with double underscore, such as `.command-button__icon`.
- Use explicit state and variant attributes, such as `[data-state~="hover"]`, `[data-state~="disabled"]`, `[data-variant="primary"]`, and `[data-variant~="vertical"]` when variants can be combined.
- Do not use type selectors, global selectors, id selectors, descendant chains that expose implementation structure, browser pseudo-classes, or project-specific feature ids.

Use logical geometry in CSS notation. Prefer `inline-size`, `block-size`, `padding-inline`, `padding-block`, `inset-inline`, and `inset-block` over physical `width`, `height`, `left`, `right`, `top`, and `bottom` unless the widget specifically requires physical direction.

CSS variables reference local UI role defaults by selector scope:

- A root declaration such as `.command-button { --height: 32px; }` is referenced as `var(--height)` in `.command-button`.
- A part declaration such as `.command-button__icon { --size: 16px; }` is referenced as `var(--size)` in `.command-button__icon`.
- A state declaration such as `.command-button[data-state~="hover"] { --background: #eef2f7; }` is referenced as `var(--background)` in that state selector.
- A part-state declaration such as `.context-menu__row[data-state~="hover"] { --background: #eef2f7; }` is referenced as `var(--background)` in that part-state selector.

Every CSS variable that affects the default visual or layout result must correspond to a `# UI Roles` custom-property fallback in the same selector scope or an inherited selector scope, an inherited platform value, a documented environment value, or a documented dynamic widget-state value.

Allowed environment values are `available-inline-size`, `available-block-size`, and `max-label-inline-size`. Allowed helper functions are `measure("M", <font-size>, <font-weight>)` for font-derived row metrics and ordinary CSS math functions such as `calc()`, `min()`, `max()`, and `clamp()`.

Allowed dynamic widget-state values are `--hold-progress` for hold-to-confirm progress from `0` to `1`.

If a CSS block contradicts prose, anatomy, state, variant, interaction, layout, or UI role sections, the spec is invalid. Fix the contradiction instead of choosing one source.
