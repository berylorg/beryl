# Name

Canonical name: thread selector trigger

Sometimes known as: active-thread selector, thread-switcher trigger

# Purpose

Presents the selected thread as one stretchable toolbar control with a stable trailing flyout affordance and explicit loading, open, and unavailable states.

# References

Contracts:

- beryl-command-geometry
- disabled-command-tooltip
- expected-action-availability

Widgets:

- tooltip

# Anatomy

The thread selector trigger contains one command-capable root, primary selected-thread title region, trailing flyout label, and trailing disclosure glyph. The complete root is one activation target; the title and trailing affordance are not separate commands.

The owning feature supplies the selected-thread identity, bounded visible title or fallback, bounded
accessible name, flyout label, activation command, readiness, unavailability reason, and associated
popup state. The widget owns trigger geometry, title truncation, focus, command feedback, and stable
trailing-affordance placement.

The widget does not contain runtime, root, catalog, thread status, metadata actions, or flyout content.

# Look

The trigger reads as the toolbar's stretchable active-thread control. The title takes available space and truncates before the trailing flyout affordance moves or disappears.

The trailing label and disclosure glyph remain visually grouped. Open state connects the control visually to its associated flyout without changing control size.

Loading dims the complete control while retaining its last coherent title or owner-supplied placeholder. Unavailable state remains visibly distinct and explanatory.

# States

The widget supports ready, hover, pressed, focused, closed, open, loading, unavailable, title present, fallback title, and truncated states.

Loading is inert rather than command-disabled presentation: it rejects focus and activation while the catalog snapshot is not ready. Unavailable is a visible disabled command state with an owner-supplied explanation.

# Interaction

Pointer activation or focused Enter and Space invoke the owner-supplied open command when ready. The command is dispatched once for the exact selected-thread and readiness revision represented by the control.

Opening the associated flyout sets open state without replacing the trigger. Dismissing the flyout returns focus to this exact trigger. A successful activation that moves focus to another window-level target follows the owning feature's focus rule.

Loading rejects pointer, keyboard, touch, and programmatic activation and does not acquire focus. Catalog readiness changes the same widget instance to ready state.

Unavailable remains focusable for inspection but never invokes its command. It satisfies `disabled-command-tooltip` with the closest owner-supplied actionable reason.

When the title truncates geometrically, hover or focus exposes the complete owner-supplied bounded
title projection through `tooltip`. Title updates retain trigger focus and popup anchoring because
root identity follows the main-window trigger instance, not the displayed title.

While the associated flyout is open, selected-thread title updates occur in place and do not move or recreate the anchor. If the selected-thread identity changes through successful activation, the associated old flyout closes before the new identity is published.

Content-free diagnostics expose widget instance id, selected-thread identity presence, readiness revision, state family, focus presence, open-popup presence, title truncation presence, and tooltip-anchor presence. Diagnostics never include thread titles, paths, runtime names, unavailability text, or tooltip content.

# Layout

The root fills the toolbar's remaining inline allocation between leading navigation controls and trailing window commands. It obeys `beryl-command-geometry` for toolbar height and focus treatment.

The title region flexes and may shrink to zero after preserving the control's accessible name. The trailing flyout label and disclosure glyph remain fixed-size and aligned to the trailing edge.

State transitions do not change block size, outer padding, trailing-affordance width, or neighboring toolbar geometry.

Spec CSS:

```css
.thread-selector-trigger {
  display: flex;
  align-items: center;
  box-sizing: border-box;
  min-inline-size: 0;
  inline-size: 100%;
  block-size: var(--height);
  padding-inline: var(--padding-x);
  gap: var(--gap);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
}

.thread-selector-trigger__title {
  flex: 1 1 auto;
  min-inline-size: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
}

.thread-selector-trigger__flyout-affordance {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: var(--affordance-gap);
  white-space: nowrap;
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
}

.thread-selector-trigger[data-state~="hover"],
.thread-selector-trigger[data-state~="pressed"] {
  background: var(--background);
  border-color: var(--border-color);
}

.thread-selector-trigger[data-state~="focused"] {
  outline: var(--ring-width) solid var(--ring-color);
  outline-offset: var(--ring-offset);
}

.thread-selector-trigger[data-state~="open"] {
  background: var(--background);
  color: var(--foreground);
}

.thread-selector-trigger[data-state~="loading"],
.thread-selector-trigger[data-state~="unavailable"] {
  opacity: var(--opacity);
}
```

# Variants

Default variant: stretchable toolbar trigger with text title and trailing flyout affordance.

# UI Roles

```css
.thread-selector-trigger {
  --height: 32px;
  --padding-x: 12px;
  --gap: 10px;
  --border-width: 1px;
  --border-color: #334155;
  --radius: 6px;
  --background: #172033;
  --foreground: #e2e8f0;
}

.thread-selector-trigger__title {
  --foreground: #f1f5f9;
  --font-size: 13px;
  --font-weight: 600;
}

.thread-selector-trigger__flyout-affordance {
  --affordance-gap: 4px;
  --foreground: #7dd3fc;
  --font-size: 10px;
  --font-weight: 700;
}

.thread-selector-trigger[data-state~="hover"] {
  --background: #1e293b;
  --border-color: #475569;
}

.thread-selector-trigger[data-state~="pressed"] {
  --background: #263449;
  --border-color: #64748b;
}

.thread-selector-trigger[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #38bdf8;
  --ring-offset: -2px;
}

.thread-selector-trigger[data-state~="open"] {
  --background: #1e293b;
  --foreground: #f8fafc;
}

.thread-selector-trigger[data-state~="loading"] {
  --opacity: 0.55;
}

.thread-selector-trigger[data-state~="unavailable"] {
  --opacity: 0.68;
}
```
