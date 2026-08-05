---
name: gui
description: Unified GUI documentation and implementation contracts, terminology, integration slots, feature GUI mounts, reusable widget specs, and built-in widget references. Use when naming, designing, implementing, reviewing, or documenting GUI windows, slots, feature GUI composition, widget specs, project-local widgets under doc/gui/widgets, external GUI spec registries, built-in controls, UI roles, widget CSS notation, or reusable GUI contracts.
---

# GUI

## Core Rule

Use this skill for GUI implementation authority, GUI documentation structure, GUI terminology, reusable widget contracts, built-in widget references, project-local widget specs, external GUI spec registries, integration slots, and feature GUI mount descriptions.

This skill owns the process, schemas, and bundled reference specs. The project repository owns project-specific GUI facts in `doc/gui/` and `doc/features/<feature>/gui.md`.

Product behavior remains owned by `doc/features/<feature>/design.md`. GUI docs may describe composition, placement, layout, widget dependencies, and widget-local interaction mechanics, but they must not make feature behavior authority ambiguous.

Do not introduce formal slot or mount parameters beyond the section shapes in this skill unless the operator explicitly approves a schema extension.

## Implementation Authority

When implementing or reviewing project GUI code, treat `doc/gui/` and linked `doc/features/<feature>/gui.md` files as authoritative GUI contracts.

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
3. Inventory each visible control and nontrivial composite in the requested GUI, then classify it as
   built-in, project-local reusable, externally registered, or feature-local before writing
   composition prose.
4. Create or select required reusable widget specs before describing how a feature configures and
   mounts those widgets.
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

## Conditional Authoring References

Before creating, editing, reviewing, or implementing against `doc/gui/integration.md`, `doc/features/<feature>/gui.md`, or `doc/gui/external-specs.md`, read [project GUI authoring](references/project-gui-authoring.md). It owns the exact window, slot, mount, and external-registry schemas and constraints.

Before creating, editing, reviewing, or implementing against reusable widget specs or reusable contracts, read [widget authoring](references/widget-authoring.md). It owns dependency declarations, required document structures, and section semantics.

Before creating, editing, reviewing, or implementing UI roles or widget `Spec CSS`, read [widget styling authoring](references/widget-styling.md). It owns UI-role addressing, exact-value placement, selector restrictions, fallback coverage, and CSS notation. Read it together with the widget-authoring reference when the task creates or reviews a styled widget spec.

Read the project-GUI and widget-authoring references when a task spans project GUI composition and reusable widget contracts or specs. Also read the widget-styling reference when that task authors or interprets UI roles or `Spec CSS`. Classification and terminology work that does not create, review, or implement against those artifacts does not require an authoring reference.

## Feature GUI Composition Boundary

Keep product workflows, permissions, persistence rules, disabled/error behavior, and acceptance rules in the feature `design.md`. Keep layout, widget composition, visual grouping, and slot mounting in `gui.md`.

A feature GUI document configures and composes canonical widgets. It must not substitute for a
widget spec by defining a control's reusable anatomy, general state model, generic interaction
rules, focus model, layout algorithm, virtualization behavior, variants, or UI roles.

Feature GUI prose may define feature-specific labels, content, ordering, data meaning, command
placement, mode selection, and relationships between canonical widgets. When that prose needs a
new stable composite control identity or reusable contract, create a project-local widget spec first
and refer to it by canonical name.

A feature-local arrangement is a one-off composition of existing canonical widgets that introduces
no new reusable control identity, state machine, generic interaction contract, layout algorithm,
virtualization behavior, variant family, or UI roles. If a nontrivial composition remains
feature-local, state that classification and its reason explicitly in the feature GUI document.

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

Create a project-local widget spec when the project intentionally adapts or overrides an existing
built-in or external spec. Otherwise, create one when no built-in or external spec covers the
control and any of these conditions applies:

- The same composite anatomy is used by two or more commands, modes, variants, or mounted surfaces,
  even inside one feature.
- The composite has a stable identity while internal content or collections change.
- The composite owns focus entry or return, keyboard traversal, selection, open or close,
  commit or cancel, or another interaction model beyond its child widgets.
- The composite owns scrolling, virtualization, overscan, anchor preservation, or transient-panel
  preservation.
- The composite needs named anatomy, a state family, layout rules, variants, or UI roles not already
  supplied by its child widgets.
- The operator explicitly requests a widget or reusable GUI contract.

Do not classify a composite as feature-local merely because all current uses belong to one feature.
Feature-local composition describes how existing widgets are arranged and configured; it is not an
alternate location for a widget contract.

During review, inspect feature GUI prose for definitions that belong under widget-spec sections
such as Anatomy, States, Interaction, Layout, Variants, or UI Roles. Extract those definitions into
a widget spec and leave only feature-specific configuration and composition in the feature GUI doc.

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

## Widget Authoring Boundary

The conditional widget-authoring references own the required contract and widget-spec structures, direct dependency format, section semantics, UI-role addressing, and CSS notation. Do not create or reinterpret those schemas from this summary.
