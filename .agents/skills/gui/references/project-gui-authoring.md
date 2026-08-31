# Project GUI Authoring

These rules are normative when creating, editing, reviewing, or implementing against GUI integration slots, feature GUI mounts, or the external widget registry.

## Integration Slots

`doc/gui/integration.md` declares the project GUI windows and the slots into which feature-owned GUI can mount.

Keep this file limited to project-specific window contracts and slot declarations. Do not repeat the GUI skill's generic explanation of slots, mounts, feature GUI docs, or widget documentation.

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
