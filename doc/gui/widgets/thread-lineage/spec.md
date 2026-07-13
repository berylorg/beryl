# Name

Canonical name: thread lineage

Sometimes known as: lineage strip, thread breadcrumb trail

# Purpose

Presents ordered parent-thread navigation and the current-thread endpoint in one fixed-height strip with bounded horizontal overflow.

# References

Contracts:

- disabled-command-tooltip
- scroll-ownership

Widgets:

- command button
- scrollbar
- tooltip

# Anatomy

The thread lineage contains a fixed root strip, structural heading, horizontal trail viewport, windowed trail layer, ordered parent breadcrumb controls, separators, readonly current-thread endpoint, and an overlay horizontal scrollbar.

Each parent breadcrumb has an owner-supplied stable thread identity, title, availability state, accessible context, and navigation command. The current endpoint has stable identity and title but no activation command.

The widget owns strip anatomy, breadcrumb geometry, focus movement, truncation, horizontal overflow, bounded realization, unavailable/current presentation, and tooltip anchoring. The owning feature supplies lineage order, labels, availability meaning, navigation effects, and whether the widget is mounted.

# Look

The strip reads as lightweight navigation chrome subordinate to the toolbar. The structural heading stays visually quiet while parent breadcrumbs read as compact controls and the current endpoint reads as a readonly destination.

Separators remain visually compact and never become focus targets. Long labels truncate within capped breadcrumb widths. Tooltips and accessibility output expose complete owner-supplied titles and unavailable explanations.

Horizontal overflow does not wrap the trail, increase strip height, or create outer-window scrolling.

# States

The widget supports ready, horizontal overflow, leading clamped, trailing clamped, manually scrolled, auto-revealing current, focused breadcrumb, tooltip visible, reconciling, and inert states.

Parent breadcrumbs support normal, hover, pressed, focused, unavailable, open elsewhere, truncated, and navigation-pending states. The current endpoint supports current and truncated states only.

# Interaction

Activating an available parent breadcrumb reports its exact stable thread identity and command to the owner. Enter and Space activate the focused breadcrumb. Unavailable breadcrumbs remain represented, do not activate through pointer, keyboard, or programmatic paths, and satisfy `disabled-command-tooltip`.

Left and Right move focus through every parent breadcrumb in lineage order, including unavailable breadcrumbs that expose an explanation. Home and End focus the first and last parent breadcrumb. Focus movement reveals the complete focused control geometry without activating it.

The trail viewport owns horizontal wheel, touchpad, Shift-plus-wheel, scrollbar drag, and programmatic reveal while it has overflow. Boundary propagation follows `scroll-ownership`. Vertical wheel intent is not converted to horizontal motion unless the platform or gesture explicitly supplies horizontal intent.

On first mount and after a selected-thread identity change, the viewport reveals the current endpoint at the trailing edge. Manual horizontal scrolling detaches that automatic placement until another selected-thread identity is published.

The trail uses variable-width horizontal windowing. Each breadcrumb width is clamped between the widget minimum and maximum. The widget realizes items intersecting the viewport plus at most two complete breadcrumb items before and two after the visible range; separators are realized with their following item.

Stable thread identity, not visible index, owns focus, width measurement, navigation dispatch, and tooltip anchoring. A reconciliation that retains the focused breadcrumb preserves focus and reveals it. If the focused identity disappears, focus returns to the trail viewport without activating another breadcrumb.

If windowing removes a tooltip-owning item, the tooltip closes intentionally. The widget never retains unbounded offscreen breadcrumbs solely to preserve focus, hover, or popup geometry.

Content-free diagnostics expose widget instance id, opaque nonreversible selected-thread and breadcrumb diagnostic keys, lineage count, realized item count, visible diagnostic-key range, overscan item count, measured-width count, viewport width, content width, scroll offset, clamp direction, focused-key presence, current endpoint presence, and tooltip-anchor presence. Diagnostics never include titles, paths, availability explanations, raw thread ids, or tooltip text.

# Layout

The root fills `main-window.thread-lineage` and keeps one fixed block size. The structural heading is fixed at the leading edge. The trail viewport receives all remaining inline space and clips overflow.

Breadcrumbs and separators occupy one horizontal row. Each breadcrumb has a capped content width and one fixed block size. The current endpoint may use the remaining inline allocation but remains width-capped when the complete trail overflows.

The horizontal scrollbar overlays the viewport's bottom edge and does not change strip height when overflow begins or ends.

Spec CSS:

```css
.thread-lineage {
  display: flex;
  align-items: center;
  box-sizing: border-box;
  min-inline-size: 0;
  inline-size: 100%;
  block-size: var(--height);
  padding-inline: var(--padding-x);
  gap: var(--gap);
  border-block-end: var(--border-width) solid var(--border-color);
  background: var(--background);
  color: var(--foreground);
}

.thread-lineage__heading {
  flex: none;
  white-space: nowrap;
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
  letter-spacing: var(--letter-spacing);
}

.thread-lineage__viewport {
  position: relative;
  flex: 1 1 auto;
  min-inline-size: 0;
  block-size: 100%;
  overflow: hidden;
}

.thread-lineage__trail {
  display: flex;
  align-items: center;
  min-inline-size: max-content;
  block-size: 100%;
  gap: var(--trail-gap);
  white-space: nowrap;
}

.thread-lineage__breadcrumb,
.thread-lineage__current {
  min-inline-size: var(--breadcrumb-min-width);
  max-inline-size: var(--breadcrumb-max-width);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.thread-lineage__breadcrumb {
  color: var(--foreground);
}

.thread-lineage__current {
  color: var(--foreground);
  font-weight: var(--font-weight);
}

.thread-lineage__separator {
  flex: none;
  color: var(--foreground);
}

.thread-lineage[data-state~="inert"] {
  opacity: var(--opacity);
}
```

# Variants

Default variant: ordered parent breadcrumbs with a readonly current endpoint.

# UI Roles

```css
.thread-lineage {
  --height: 32px;
  --padding-x: 12px;
  --gap: 10px;
  --border-width: 1px;
  --border-color: #334155;
  --background: #0f172a;
  --foreground: #cbd5e1;
}

.thread-lineage__heading {
  --foreground: #64748b;
  --font-size: 10px;
  --font-weight: 700;
  --letter-spacing: 0.8px;
}

.thread-lineage__trail {
  --trail-gap: 6px;
}

.thread-lineage__breadcrumb,
.thread-lineage__current {
  --breadcrumb-min-width: 48px;
  --breadcrumb-max-width: 220px;
}

.thread-lineage__breadcrumb {
  --foreground: #7dd3fc;
}

.thread-lineage__breadcrumb[data-state~="unavailable"] {
  --foreground: #64748b;
}

.thread-lineage__current {
  --foreground: #e2e8f0;
  --font-weight: 600;
}

.thread-lineage__separator {
  --foreground: #64748b;
}

.thread-lineage[data-state~="inert"] {
  --opacity: 0.55;
}
```
