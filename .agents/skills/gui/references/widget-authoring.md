# Widget Authoring

These rules are normative when creating, editing, reviewing, or implementing against reusable widget specs, reusable widget contracts, UI roles, or widget `Spec CSS`.

## Contents

- Dependency references
- Required contract structure
- Project widget specs
- Required spec structure

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

Use `# Layout` for geometry and placement rules, including how the widget behaves in constrained space. Use semantic geometry, formulas, relative relationships, constrained-space behavior, and owner responsibilities; put literal default sizes, spacing, offsets, minimums, maximums, and placement constants in `# UI Roles`.

Use `# Variants` only for deliberate widget variants. Include one `Default variant:` line when the widget has more than one variant or when the default needs to be explicit.

Use `# UI Roles` to define CSS custom-property fallbacks for all visual-impacting and layout-impacting parameters in the default variant. Use selectors that map to widget anatomy and state. This is the only widget-spec section where exact default visual and layout fallback values belong.
