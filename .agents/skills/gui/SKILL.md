---
name: gui
description: Unified GUI documentation and implementation contracts, terminology, integration slots, feature GUI mounts, reusable widget specs, and built-in widget references. Use when naming, designing, implementing, reviewing, or documenting GUI windows, slots, feature GUI composition, widget specs, project-local widgets under doc/gui/widgets, external GUI spec registries, built-in controls, UI roles, widget CSS notation, or reusable GUI contracts.
---

# GUI

## Core Rule

Use this skill for GUI implementation authority, GUI documentation structure, GUI terminology, reusable widget contracts, built-in widget references, project-local widget specs, external GUI spec registries, integration slots, and feature GUI mount descriptions.

This skill owns the process, schemas, and bundled reference specs. The project repository owns project-specific GUI facts in `doc/gui/` and `doc/features/<feature>/gui.md`.

Product behavior remains owned by `doc/features/<feature>/design.md`. GUI docs may describe composition, placement, layout, widget dependencies, and visual interaction details, but they must not make feature behavior authority ambiguous.

Do not introduce formal slot or mount parameters beyond the section shapes in this skill unless the operator explicitly approves a schema extension.

## Implementation Authority

When implementing or reviewing Beryl GUI code, treat `doc/gui/` and linked `doc/features/<feature>/gui.md` files as authoritative GUI contracts.

GUI implementation must conform to:

- `doc/gui/integration.md` for OS windows, top-level layout, and slot existence.
- `doc/features/<feature>/gui.md` for feature-owned mounted GUI composition.
- `doc/gui/widgets/<widget-name>/spec.md` for project-local reusable widgets.
- `doc/gui/widgets/contracts/<contract-name>.md` for project-local reusable GUI contracts.
- Built-in widget specs bundled with this skill.
- External widget specs registered in `doc/gui/external-specs.md`.

If implementation needs GUI behavior, layout, slots, or reusable widget semantics not covered by those docs, update the authoritative GUI docs first.

Feature `design.md` files still own product behavior, workflows, permissions, persistence rules, disabled and error behavior, and acceptance rules.

## Workflow

1. Read project documentation authority before changing GUI docs.
2. Use `references/terminology.md` when naming common GUI concepts.
3. Inventory each visible control and nontrivial composite in the requested GUI, then classify it as built-in, project-local reusable, externally registered, or feature-local before writing composition prose.
4. Create or select required reusable widget specs before describing how a feature configures and mounts those widgets.
5. Put window and slot declarations in `doc/gui/integration.md`.
6. Put feature-owned GUI composition in `doc/features/<feature>/gui.md`.
7. Put reusable project widget specs under `doc/gui/widgets/<widget-name>/spec.md`.
8. Put reusable project widget contracts under `doc/gui/widgets/contracts/<contract-name>.md`.
9. Put external visible widget registries in `doc/gui/external-specs.md`.

## Project GUI Locations

Use these project-owned locations:

- `doc/gui/integration.md` for windows and slot declarations.
- `doc/gui/external-specs.md` for externally owned GUI specs visible in the project.
- `doc/gui/widgets/<widget-name>/spec.md` for project-local reusable widget specs.
- `doc/gui/widgets/contracts/<contract-name>.md` for project-local reusable GUI contracts.
- `doc/features/<feature>/gui.md` for feature-owned GUI composition and mount declarations.

Only use the paths listed above for new project-local widget specs and contracts.

## Integration Slots

`doc/gui/integration.md` declares the project GUI windows and the slots into which feature-owned GUI can mount.

Keep this file limited to project-specific window contracts and slot declarations. Do not repeat this skill's generic explanation of slots, mounts, feature GUI docs, or widget documentation.

Use this exact top-level shape:

```markdown
# GUI Integration

# Windows

## Main Workspace Window

The main workspace window is ...

### Slots

#### Slot: main-window.thread-strip

This slot is ...
```

Every slot section must be nested under the OS window that contains it. Do not use a global `# Slots` section.

Each window section body should define only the contract owned by that OS window: its purpose, top-level layout, sizing rules, scrolling policy, chrome ownership, and other constraints that feature-owned mounted GUI must respect.

Introduce region terms where they are first needed in the relevant window or slot section. Do not create a standalone glossary section such as `# Shared Region Terms`.

A slot declaration establishes that a named insertion point exists within its window. It describes where the slot is, what role inserted GUI plays, and any surrounding layout constraints that belong to the integration skeleton. It does not decide which feature fills the slot.

The slot id is a stable dotted name. Prefer the owning window prefix followed by a short slot name, such as `main-window.thread-strip` or `settings-window.body`. If a globally integrated slot is inside a feature-owned container within a window, declare it under the containing OS window even when the stable slot id uses the feature container prefix.

The slot body is plain Markdown prose. There are no required formal fields inside a slot section.

Do not list mounted features in `doc/gui/integration.md`. The feature GUI doc owns the mount declaration.

Do not add general explanatory sections such as `# Window Contract`, `# Main Workspace Window Layout`, or prose that restates that feature GUI docs mount with `Mount-into:`. Put window sizing and layout contracts under the relevant window section.

## Feature GUI Mounts

Use `doc/features/<feature>/gui.md` when a feature needs substantial GUI layout, widget composition, or slot mounting detail.

Link the `gui.md` file from the feature's `design.md` as normative supplemental material when it exists.

The mounted UI section title is the name of the UI thing being described. Put the mount field directly under that heading:

```markdown
## Thread Strip

Mount-into: main-window.thread-strip

This UI contains thread creation, back and forward navigation, and active thread selection.
```

`Mount-into:` is the only required formal field for a mounted feature GUI section. Its value must exactly match a slot declared in `doc/gui/integration.md`.

Do not wrap mounted UI sections in a generic `# Mounts` section. After the document's top-level GUI heading, use the mounted UI section headings directly.

If a feature GUI needs a slot that does not exist, update `doc/gui/integration.md` first. If a feature-owned container needs to expose a mount point for other features, promote that slot into `doc/gui/integration.md` before other feature docs reference it.

Keep product workflows, permissions, persistence rules, disabled/error behavior, and acceptance rules in the feature `design.md`. Keep layout, widget composition, visual grouping, and slot mounting in `gui.md`.

A feature GUI document configures and composes canonical widgets. It must not substitute for a widget spec by defining a control's reusable anatomy, general state model, generic interaction rules, focus model, layout algorithm, virtualization behavior, variants, or UI roles.

Feature GUI prose may define feature-specific labels, content, ordering, data meaning, command placement, mode selection, and relationships between canonical widgets. When that prose needs a new stable composite control identity or any reusable contract named above, create a project-local widget spec first and refer to it by canonical name.

A feature-local arrangement is limited to a one-off composition of existing canonical widgets that introduces no new reusable control identity, state machine, generic interaction contract, layout algorithm, virtualization behavior, variant family, or UI roles. If a nontrivial composition remains feature-local, state that classification and its reason explicitly in the feature GUI document.

## External Spec Registry

`doc/gui/external-specs.md` registers canonical widget names that are visible to the current project but owned outside the project repository.

Use one section per external source. The section title identifies the source.

Use this section shape:

```markdown
## gpui-scrollbar

Code dependency: Cargo crate `gpui-scrollbar`

Spec root: ../gpui-scrollbar/doc/gui/widgets

Canonical widgets:

- scrollbar
```

`Code dependency:` names how the code reaches the external implementation.

`Spec root:` is a local documentation lookup path for agents and developers. It is not a Cargo mechanism and does not imply that Cargo can locate the docs.

`Canonical widgets:` lists the widget names that project docs may reference as externally provided.

If an external spec path is unavailable, report the missing external spec. Do not reconstruct the external widget contract from code unless the operator explicitly asks for that investigation.

## Terminology Catalog

Use `references/terminology.md` as the baseline vocabulary catalog.

Consult the catalog when naming GUI elements, reviewing UI docs, resolving ambiguous widget names, or deciding whether a custom widget spec is needed. Prefer catalog terms unless the project has an explicit reason to introduce a local term.

Keep the catalog limited to broadly established GUI terminology. Do not add project-specific widget names or predefined widget spec names merely because a spec file exists.

## Widget Dependency Resolution

Before creating a new widget spec, classify the widget as one of these:

- Built-in reference from this skill.
- Project-local reusable widget under `doc/gui/widgets/<widget-name>/spec.md`.
- Externally registered widget from `doc/gui/external-specs.md`.
- Feature-local arrangement that does not need a reusable widget spec.

Use canonical widget names in docs and dependency lists.

Create a project-local widget spec when no built-in or external spec covers the control and any of these conditions applies:

- The same composite anatomy is used by two or more commands, modes, variants, or mounted surfaces, even inside one feature.
- The composite has a stable identity while internal content or collections change.
- The composite owns focus entry/return, keyboard traversal, selection, open/close, commit/cancel, or another interaction model beyond its child widgets.
- The composite owns scrolling, virtualization/windowing, overscan, anchor preservation, or popover/tooltip preservation.
- The composite needs named anatomy, a state family, layout rules, variants, or UI roles that are not already supplied by its child widgets.
- The operator explicitly requests a widget or reusable GUI contract.

Do not classify a composite as feature-local merely because all current uses belong to one feature. Feature-local composition describes how existing widgets are arranged and configured; it is not an alternate location for a widget contract.

During review, inspect feature GUI prose for definitions that belong under widget-spec sections such as Anatomy, States, Interaction, Layout, Variants, or UI Roles. Extract those definitions into a widget spec and leave only feature-specific configuration and composition in the feature GUI document.

## Built-In Widget Specs

Use these bundled specs when a project needs one of the predefined widget patterns:

- `references/widget-specs/command-button/spec.md`
- `references/widget-specs/single-line-text-field/spec.md`
- `references/widget-specs/multiline-text-field/spec.md`
- `references/widget-specs/segmented-status-bar/spec.md`
- `references/widget-specs/context-menu/spec.md`
- `references/widget-specs/anchored-context-menu/spec.md`
- `references/widget-specs/hold-to-confirm-button/spec.md`
- `references/widget-specs/scrollbar/spec.md`
- `references/widget-specs/tooltip/spec.md`

Treat these specs as bundled skill references. Do not copy them into project docs unless the project intentionally adapts or overrides the widget contract as a project-local custom widget.

## Reusable Contracts

Use these bundled contracts when a project needs one of the predefined reusable widget obligations:

- `references/contracts/disabled-command-tooltip.md`

Contracts are reusable behavioral, dependency, or state obligations. They are not complete widgets and usually do not define CSS.

Contracts may reference concrete widget specs by canonical widget name when satisfying the contract requires a concrete UI element. Treat each contract reference to a widget as an explicit dependency.

Widget specs may reference contracts and other widgets. Authors should avoid circular dependencies. If a cycle appears, resolve it during review by moving shared behavior into a lower contract or by removing an unnecessary dependency.

## Dependency References

Use `# References` in widget specs and contract docs to list direct dependencies by canonical name.

Use these dependency groups:

- `Contracts:` for reusable contract dependencies.
- `Widgets:` for concrete widget dependencies.

Write `N/A` when a spec has no direct dependencies.

List only direct dependencies, not transitive dependencies. A widget that uses `disabled-command-tooltip` lists that contract; the contract itself lists the required `tooltip` widget.

References are reviewable dependency edges. Prefer simple acyclic graphs, but do not invent vague wording to avoid naming a real dependency.

## Required Contract Structure

Every reusable contract must use these sections, in this order:

```markdown
# Name

Canonical name: <name>

# Purpose

<What reusable obligation this contract defines, or N/A.>

# References

<Direct contract and widget dependencies by canonical name, or N/A.>

# Applies To

<Which widgets, states, or situations the contract applies to, or N/A.>

# Rule

<The reusable obligation, behavior, dependency, or state rule.>
```

If a section has nothing meaningful to say, write `N/A` as that section's body. Do not omit mandatory sections.

## Project Widget Specs

Document project-local reusable widgets at:

```text
doc/gui/widgets/<widget-name>/spec.md
```

Use lowercase hyphenated directory names for `<widget-name>`.

Keep widget specs focused on the reusable widget contract. Put feature-specific workflows, product rules, permissions, data lifecycles, and visible error behavior in the owning feature design doc unless the project declares a different documentation authority.

Document project-local reusable contracts at:

```text
doc/gui/widgets/contracts/<contract-name>.md
```

Use lowercase hyphenated names for `<contract-name>`.

Keep contracts focused on reusable obligations and dependency rules. Put concrete widget anatomy, CSS, visual variants, and UI roles in widget specs unless the contract itself is the concrete renderable element.

## Required Spec Structure

Every project-local widget spec must use these sections, in this order:

```markdown
# Name

Canonical name: <name>

Sometimes known as: <other names, or N/A>

# Purpose

<What reusable UI problem this widget solves, or N/A.>

# References

<Direct contract and widget dependencies by canonical name, or N/A.>

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

Use `# Name` to establish one canonical term. Include aliases only to map common vocabulary to the canonical name.

Use `# Purpose` to explain why the widget exists as a reusable control, not what one feature does with it.

Use `# References` to list direct dependencies. Use canonical names, not file paths.

Use `# Anatomy` to name stable subparts such as trigger, label, leading icon, trailing icon, panel, item, handle, thumb, track, header, row, cell, separator, backdrop, or affordance.

Use `# Look` for visual identity and visual-state intent. Do not put exact visual or layout fallback values in this section. Use semantic descriptions such as compact, rounded, muted, inset, or thumb-only; put literal colors, dimensions, spacing, radii, opacity values, durations, and similar visual defaults in `# UI Roles`.

Use `# States` to list all user-visible widget states the implementation must represent.

Use `# Interaction` for behavior caused by user input, including hover, press, click, drag, keyboard activation, focus movement, opening panels, closing panels, committing choices, cancelling choices, and outside-click dismissal.

Use `# Layout` for geometry and placement rules, including how the widget behaves in constrained space. Do not put exact layout fallback values in this section. Use semantic geometry, formulas, relative relationships, constrained-space behavior, and owner responsibilities; put literal default sizes, spacing, offsets, minimums, maximums, and placement constants in `# UI Roles`.

Use `# Variants` only for deliberate widget variants. Include one `Default variant:` line when the widget has more than one variant or when the default needs to be explicit.

Use `# UI Roles` to define CSS custom-property fallbacks for all visual-impacting and layout-impacting parameters in the default variant. Use selectors that map to widget anatomy and state. This is the only widget spec section where exact default visual and layout fallback values belong.

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

Structural CSS literals and keywords such as `display`, `position`, `box-sizing`, `flex-direction`, `align-items`, `justify-content`, `0`, `100%`, `auto`, and overflow or wrapping keywords are allowed in `Spec CSS:` only when they express layout mechanics such as fill, origin, reset, intrinsic sizing, alignment, clipping, wrapping, or a dynamic formula rather than a tunable widget default.

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
