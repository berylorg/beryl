# Scope

Unsupported accessibility publication across the owned GPUI fork, first exposed by
`gpui-text-input` Phase 133 and encountered again by the Phase 300 notice widget.

# Invalidated Assumption

The Phase 133 widget work assumed that bounded realized-object accessibility facts could be
published as GPUI semantic nodes and receive semantic activation through the pinned GPUI public
boundary without changing the fork.

# Decisive Evidence

The pinned GPUI revision `b83f38e38839ab1b917febfbbacfbed900e57e09` exposes only layout,
prepaint, and paint through `crates/gpui/src/element.rs`. Its manifest and public modules have no
AccessKit dependency, accessibility tree, semantic-node writer, semantic property API, or
accessibility-action route. Ordinary `Div::on_action` and pointer handlers are GPUI input paths,
not accessibility adapter actions.

Phase 300 reconfirmed the missing capability at pinned revision
`b6939aa0a2d78be59facd12a909a4ada3b6f6a64`, matching the inspected sibling checkout. Its public
`Element`, `Interactivity`, and `Window` surfaces and internal `PlatformWindow` trait expose no
semantic naming or tree publication route. A `TooltipBuilder` creates only a visual hover view.

The original `gpui-text-input` draft separately retained a label and optional description, but it
could not publish them as an OS accessibility node or route an assistive-technology action through
the pinned dependency.

An experimental `origin/a11y-gpui` branch demonstrates a materially different architecture. When
accessibility is active it builds a frame-wide AccessKit tree and uses vectors, hash maps, duplicate
identity tracking, allocated labels, and a boxed-listener registry. That approach is neither in the
pinned revision nor compatible as-is with Phase 132's no-registry, no-routine-scan, and hot-rendering
path constraints.

# Why The Approach Fails

Actual semantic publication and activation cannot be implemented inside `gpui-text-input` alone.
Treating ordinary key or pointer events as accessibility actions would be a false adapter, while
copying the experimental frame-wide registry would add unplanned architectural scope and violate
the accepted performance boundary.

# Accepted Correction

The Operator rejected accessibility scope. Phase 133 removes the accessibility-specific label and
description payload, its accounting, public API, and tests. The coherent surface retains only the
visual presentation, opaque semantic state, and activation eligibility required by ordinary widget
rendering and interaction.

The Operator subsequently made accessibility an explicit project-wide [non-goal](../design.md).
Active feature and GUI contracts therefore require no assistive naming, state, or action support.
Ordinary keyboard interaction, focus management, visible labels, and tooltips remain GUI behavior.
Missing platform accessibility must not become an implementation prerequisite for those controls.

Do not add a compatibility adapter, synthetic accessibility action, unbounded registry, or
per-frame logical-source scan.

# Affected Authority

- `doc/plan.md`, Phase 133.
- `../gpui-text-input/doc/design.md`, especially the Widget Layer decision.
- `../gpui-text-input/doc/gui/widgets/text-input/spec.md`, realized-object presentation and
  interaction contracts.
- `../zed-fork/doc/design.md` remains unchanged because platform accessibility is not target scope.
