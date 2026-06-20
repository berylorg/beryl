---
name: gui-widgets
description: Shared GUI terminology catalog, predefined widget specs, and custom widget documentation rules. Use when naming, discussing, designing, reviewing, or documenting GUI elements; when choosing baseline UI vocabulary for controls, containers, overlays, state, selection, navigation, or layout; when describing built-in widget patterns such as command buttons, text fields, scrollbars, segmented status bars, or context menus; or when creating/updating custom widget specs at doc/gui-widgets/<custom_widget_name>/spec.md.
---

# GUI Widgets

## Core Rule

Use established GUI terminology when a common UI concept already has a name. Do not invent project-specific names for ordinary concepts such as buttons, checkboxes, radio buttons, tabs, sliders, menus, dialogs, text fields, splitters, tables, trees, lists, toolbars, flyouts, and popovers.

Create a custom widget spec only when the project introduces a reusable widget with local semantics, local anatomy, or a visual/interaction contract that cannot be captured by baseline terminology alone.

## Terminology Catalog

Use `references/terminology.md` as the baseline vocabulary catalog.

Consult the catalog when naming GUI elements, reviewing UI docs, resolving ambiguous widget names, or deciding whether a custom widget spec is needed. Prefer catalog terms unless the project has an explicit reason to introduce a local term.

Keep the catalog limited to broadly established GUI terminology. Do not add project-specific widget names or predefined widget spec names to the terminology catalog merely because a spec file exists.

## Predefined Widget Specs

Use these built-in specs when a project needs one of the predefined widget patterns:

- `references/widget-specs/command-button/spec.md`
- `references/widget-specs/single-line-text-field/spec.md`
- `references/widget-specs/multiline-text-field/spec.md`
- `references/widget-specs/segmented-status-bar/spec.md`
- `references/widget-specs/context-menu/spec.md`
- `references/widget-specs/anchored-context-menu/spec.md`
- `references/widget-specs/hold-to-confirm-button/spec.md`
- `references/widget-specs/scrollbar/spec.md`

These reference paths mirror the project-local custom widget layout while staying inside the skill's `references/` directory.

Treat these specs as reusable reference contracts. Copy or adapt them into a project's `doc/gui-widgets/<custom_widget_name>/spec.md` only when the project needs a project-local custom widget spec.

## UI Role Addressing

Every widget spec has one widget role name.

For project-local specs, use `<custom_widget_name>` from `doc/gui-widgets/<custom_widget_name>/spec.md`.

For predefined specs, use the final directory name under `references/widget-specs/`.

If a widget spec explicitly declares `Role name:` in `# Name`, that value overrides the path-derived role name.

Define UI role defaults inside `# UI Roles` as CSS custom-property declarations, not markdown lists. Use one fenced `css` block unless the section is `N/A`.

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
- A part state selector, such as `.context-menu__item[data-state~="hover"]`, adds the part name and then the state name.

For a `context-menu` widget, `.context-menu__item[data-state~="hover"] { --background: #eef2f7; }` expands to `context-menu.item.hover.background`.

Theme-aware apps use the expanded canonical role ids directly or through a deterministic adapter for their theme system. Apps without theming use the fallback values listed in the widget spec.

The default visual variant belongs in `# Variants`. Exact visual fallback values belong in `# UI Roles`.

Prefer `foreground` for text, icon, and stroke color; `background` for fills; `width` and `height` for rectangular dimensions; `size` only when one value intentionally controls both width and height; and `padding-x` and `padding-y` instead of ambiguous padding when axes may differ.

Every visual-impacting parameter used by the default variant must have a UI role fallback unless the value is inherited from platform behavior or deliberately non-themable.

## Widget CSS Notation

Use fenced `css` blocks as specification notation when CSS makes widget look or layout easier to read. CSS in widget specs describes intended visual output; it does not require a browser, DOM, browser cascade, or a CSS-capable implementation.

Prose remains authoritative for behavior. Do not use CSS to define activation, keyboard movement, focus routing, selection semantics, open/close policy, dismissal, data ownership, validation, persistence, or feature-specific workflow.

When a `Spec CSS:` block is present, keep `# Look` and `# Layout` prose to semantic intent, constraints CSS cannot express, and short orientation for the CSS contract. Do not duplicate CSS declarations, sizing formulas, spacing values, state colors, overflow rules, or placement formulas in prose unless the duplication is needed to disambiguate a non-CSS semantic rule.

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
- A part-state declaration such as `.context-menu__item[data-state~="hover"] { --background: #eef2f7; }` is referenced as `var(--background)` in that part-state selector.

Every CSS variable that affects the default visual result must correspond to a `# UI Roles` custom-property fallback in the same selector scope or an inherited selector scope, a named fixed widget constant in prose, an inherited platform value, a documented environment value, or a documented dynamic widget-state value.

Allowed environment values are `available-inline-size`, `available-block-size`, and `max-label-inline-size`. Allowed helper functions are `measure("M", <font-size>, <font-weight>)` for font-derived row metrics and ordinary CSS math functions such as `calc()`, `min()`, `max()`, and `clamp()`.

Allowed dynamic widget-state values are `--hold-progress` for hold-to-confirm progress from `0` to `1`.

If a CSS block contradicts prose, anatomy, state, variant, interaction, layout, or UI role sections, the spec is invalid. Fix the contradiction instead of choosing one source.

## Documentation Placement

Document custom GUI widgets at:

```text
doc/gui-widgets/<custom_widget_name>/spec.md
```

Use lowercase hyphenated directory names for `<custom_widget_name>`.

Keep widget specs focused on the reusable widget contract. Put feature-specific workflows, product rules, permissions, data lifecycles, and visible error behavior in the owning feature design doc unless the project declares a different documentation authority.

## Required Spec Structure

Every custom widget spec must use these sections, in this order:

```markdown
# Name

Canonical name: <name>

Sometimes known as: <other names, or N/A>

# Purpose

<What reusable UI problem this widget solves, or N/A.>

# Anatomy

<Named parts of the widget and their relationships, or N/A.>

# Look

<Visual form, materials, color behavior, typography, spacing, borders, icons, motion, and visual feedback, or N/A.>

# States

<Supported states such as normal, hover, pressed, focused, disabled, selected, open, loading, empty, invalid, or N/A.>

# Interaction

<Pointer, keyboard, touch, focus, open/close, selection, commit/cancel, dismissal, and activation behavior, or N/A.>

# Layout

<Sizing, alignment, wrapping, truncation, anchoring, popup placement, overflow, and responsive behavior, or N/A.>

# Variants

<Supported variants and how they differ, or N/A.>

# UI Roles

<CSS custom-property defaults for local UI roles, or N/A.>
```

If a section has nothing meaningful to say, write `N/A` as that section's body. Do not omit mandatory sections.

## Section Guidance

Use `# Name` to establish one canonical term. Include aliases only to map common vocabulary to the canonical name.

Use `# Purpose` to explain why the widget exists as a reusable control, not what one feature does with it.

Use `# Anatomy` to name stable subparts such as trigger, label, leading icon, trailing icon, panel, item, handle, thumb, track, header, row, cell, separator, backdrop, or affordance.

Use `# Look` for visual identity and visual-state intent. When `Spec CSS:` is present, keep this section high-level and do not restate CSS mechanics.

Use `# States` to list all user-visible widget states the implementation must represent.

Use `# Interaction` for behavior caused by user input, including hover, press, click, drag, keyboard activation, focus movement, opening panels, closing panels, committing choices, cancelling choices, and outside-click dismissal.

Use `# Layout` for geometry and placement rules, including how the widget behaves in constrained space. When `Spec CSS:` is present, state only semantic layout constraints or owner responsibilities that CSS does not express directly.

Use `# Variants` only for deliberate widget variants. Do not use variants to document unrelated feature-specific styling. Include one `Default variant:` line when the widget has more than one variant or when the default needs to be explicit.

Use `# UI Roles` to define CSS custom-property fallbacks for all visual-impacting parameters in the default variant. Use selectors that map to widget anatomy and state. Keep custom property names local to the selector; the global UI role addressing rules define canonical ids.

Use CSS notation to carry precise widget look and layout whenever it is clearer than prose. When CSS notation is present, prose should not restate the same visual mechanics.

## Example

````markdown
# Name

Canonical name: command button

Sometimes known as: action button, push button

# Purpose

Invokes a discrete command selected by the user.

# Anatomy

The command button contains a rectangular button body and a centered label. It may include a leading icon when the icon clarifies the command.

# Look

Rectangular command control with centered text and visible state feedback.

# States

Normal, hover, pressed, focused, disabled, and loading.

# Interaction

Clicking or tapping the button invokes its assigned command when enabled.

When focused, Enter and Space invoke the command.

Disabled buttons do not invoke their command.

# Layout

The button hugs its label by default and may fill available width when a containing layout explicitly requires it.

The CSS block defines sizing, padding, border, and state visuals.

Spec CSS:

```css
.command-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  block-size: var(--height);
  inline-size: max-content;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
}

.command-button[data-state~="hover"] {
  background: var(--background);
  border-color: var(--border-color);
}
```

# Variants

Primary, secondary, destructive, and icon-leading variants.

Default variant: secondary.

# UI Roles

```css
.command-button {
  --height: 32px;
  --padding-x: 12px;
  --padding-y: 6px;
  --radius: 6px;
  --border-width: 1px;
  --background: #f8fafc;
  --foreground: #1f2937;
  --border-color: #cbd5e1;
}

.command-button[data-state~="hover"] {
  --background: #eef2f7;
  --border-color: #94a3b8;
}

.command-button[data-state~="pressed"] {
  --background: #e2e8f0;
  --border-color: #64748b;
}

.command-button[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #2563eb;
  --ring-offset: 2px;
}
```
````
