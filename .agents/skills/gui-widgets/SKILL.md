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

Define UI role keys locally inside `# UI Roles`. Do not repeat the widget role name for each key.

Expand local UI role keys into canonical role ids with:

```text
<widget-role-name>[.<part>][.<state>].<property>
```

Use these local section rules:

- `## Root` adds no part or state prefix.
- `## Parts` adds the part name from each third-level heading.
- `## States` adds the state name from each third-level heading.
- `#### States` inside a part adds the part name, then the state name from each fifth-level heading.

For a `context-menu` widget, `## Parts` -> ``### `item` `` -> `#### States` -> ``##### `hover` `` -> ``- `background`: `#eef2f7` `` expands to `context-menu.item.hover.background`.

Theme-aware apps use the expanded canonical role ids directly or through a deterministic adapter for their theme system. Apps without theming use the fallback values listed in the widget spec.

The default visual variant belongs in `# Variants`. Exact visual fallback values belong in `# UI Roles`.

Prefer `foreground` for text, icon, and stroke color; `background` for fills; `width` and `height` for rectangular dimensions; `size` only when one value intentionally controls both width and height; and `padding-x` and `padding-y` instead of ambiguous padding when axes may differ.

Every visual-impacting parameter used by the default variant must have a UI role fallback unless the value is inherited from platform behavior or deliberately non-themable.

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

<Local UI role keys with fallback values for visual-impacting parameters, or N/A.>
```

If a section has nothing meaningful to say, write `N/A` as that section's body. Do not omit mandatory sections.

## Section Guidance

Use `# Name` to establish one canonical term. Include aliases only to map common vocabulary to the canonical name.

Use `# Purpose` to explain why the widget exists as a reusable control, not what one feature does with it.

Use `# Anatomy` to name stable subparts such as trigger, label, leading icon, trailing icon, panel, item, handle, thumb, track, header, row, cell, separator, backdrop, or affordance.

Use `# Look` for visual description. Include state-dependent visual changes when they are purely visual feedback.

Use `# States` to list all user-visible widget states the implementation must represent.

Use `# Interaction` for behavior caused by user input, including hover, press, click, drag, keyboard activation, focus movement, opening panels, closing panels, committing choices, cancelling choices, and outside-click dismissal.

Use `# Layout` for geometry and placement rules, including how the widget behaves in constrained space.

Use `# Variants` only for deliberate widget variants. Do not use variants to document unrelated feature-specific styling. Include one `Default variant:` line when the widget has more than one variant or when the default needs to be explicit.

Use `# UI Roles` to list local UI role keys and fallback values for all visual-impacting parameters in the default variant. Use `## Root`, `## Parts`, and `## States` subsections as needed. Keep keys local to the widget; the global UI role addressing rules define canonical ids.

## Example

```markdown
# Name

Canonical name: command button

Sometimes known as: action button, push button

# Purpose

Invokes a discrete command selected by the user.

# Anatomy

The command button contains a rectangular button body and a centered label. It may include a leading icon when the icon clarifies the command.

# Look

Rectangular with centered text inside. Size generally hugs the button label.

The rectangle is filled with a color that acts as the button background for visual weight and label contrast.

The rectangle has a border.

On hover, one or more color elements may change.

On press, one or more color elements may change for visual feedback while activation is held, then return when released.

# States

Normal, hover, pressed, focused, disabled, and loading.

# Interaction

Clicking or tapping the button invokes its assigned command when enabled.

When focused, Enter and Space invoke the command.

Disabled buttons do not invoke their command.

# Layout

The button hugs its label by default and may fill available width when a containing layout explicitly requires it.

# Variants

Primary, secondary, destructive, and icon-leading variants.

Default variant: secondary.

# UI Roles

## Root

- `height`: `32px`
- `padding-x`: `12px`
- `padding-y`: `6px`
- `radius`: `6px`
- `border-width`: `1px`
- `background`: `#f8fafc`
- `foreground`: `#1f2937`
- `border-color`: `#cbd5e1`

## States

### `hover`

- `background`: `#eef2f7`
- `border-color`: `#94a3b8`

### `pressed`

- `background`: `#e2e8f0`
- `border-color`: `#64748b`

### `focused`

- `ring-width`: `2px`
- `ring-color`: `#2563eb`
- `ring-offset`: `2px`
```
