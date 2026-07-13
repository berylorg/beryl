# Name

Canonical name: theme editor

Sometimes known as: theme role editor, appearance role editor

# Purpose

Presents Beryl's theme-role navigator and the selected role's editable property rows as one settings-page composite.

The widget owns reusable navigator selection, nested scroll routing, bounded row realization, focus continuity, and the spatial relationship between role navigation and property editing. The host owns theme schema meaning, supported properties, inheritance and value-source semantics, draft values, validation, preview, repository mutations, Save, and Save As.

# References

Contracts:

- scroll-ownership

Widgets:

- scrollbar
- settings-row
- settings-window

# Anatomy

The theme editor contains a root, bounded role-navigator region, horizontal column-trail viewport, one column for each host-supplied selected-path level, one vertically scrollable role-row viewport per column, and a selected-role property region composed from external `settings-row` widgets.

Each navigator column contains a stable column header and role rows. Every role row has an owner-supplied stable role id, label, optional resolved visual sample, selected-path state, selected-role state, child-availability state, and supported-property count. Synthetic folders or grouping rows are not part of the widget anatomy.

The selected-role property region contains an owner-supplied selected-role heading and zero or more property rows. The widget arranges those rows but does not replace their external field, validation, modified, popup, or action mechanics.

The widget consumes a host-supplied bounded projection containing only the current selected path, its visible child columns, and the selected role's supported property rows. It never discovers the theme schema, resolves inheritance, or builds theme drafts itself.

# Look

The role navigator appears as a bounded bordered panel above the property region. Its columns read as a left-to-right path through the role schema, with stable widths, compact headers, small row gaps, and a clear selected-row treatment.

Resolved visual samples may help distinguish roles, but labels and selected state remain sufficient when a sample is absent. Static-parent or value-source meaning is not encoded only through color.

Role rows with children use a right-facing thick triangle affordance visually matched to the settings choice-control family. Leaf rows reserve no false child-navigation affordance.

The property region uses the normal external settings-row presentation so theme properties remain visually consistent with other Beryl settings. A selected role with no supported properties retains its heading and an owner-supplied empty explanation without manufacturing fallback rows.

# States

The widget supports ready, empty projection, selected role, selected path, no supported properties, same-projection refresh, selection transition, navigator focused, property field focused, navigator horizontal overflow, column vertical overflow, and inert states.

Role rows support ordinary, selected path, selected role, focused, hover, pressed, has children, leaf, sample present, and sample absent states.

Property-row modified, invalid, popup-open, and field-focus states are supplied by the referenced `settings-row` widget. The host supplies their meaning and messages.

# Interaction

Activating a role row reports its stable role id to the host. The host publishes the replacement selected-path and property projection as one same-page update; the widget does not optimistically synthesize child columns or property rows.

Keyboard and pointer selection use stable role identity rather than visible row index. A role selection keeps focus on the activated role row after the new projection arrives. If a same-projection refresh retains a focused role or property field, focus remains on that stable id. If an update removes the focused target, focus returns to the selected role row; any popup anchored to the removed target closes intentionally.

The horizontal column-trail viewport owns horizontal scrolling. Each role column owns its vertical scrolling. The selected-role property region does not create another vertical scroll container; its rows participate in the external settings-window page-body scroll. Pointer-wheel and touchpad routing follows `scroll-ownership`, and one gesture never scrolls a role column and the settings page together.

Each role column realizes a fixed-height visible row window plus bounded overscan. Stable role ids own row focus, selected state, measurement, samples, and pointer anchors across window changes. A column that scrolls a focused or popup-owning row outside its realized window closes the popup and moves focus to the column viewport rather than retaining unbounded offscreen rows.

The host-provided column trail contains only the current selected path and its next child column. Its column count is bounded by the finite hardcoded schema depth, not by installed-theme or user-authored data. The selected-role property projection contains only hardcoded supported properties and must fit the external settings-window detail-row bound; the widget does not paginate or silently omit property rows.

Same-page draft updates preserve navigator scroll positions, selected role, stable property-field focus, and compatible settings-row popups. A role change preserves column scroll positions only for columns with the same stable parent-role key and resets newly introduced columns to their selected row.

Content-free diagnostics expose widget instance id, projection revision, total schema-role count, selected-path depth, column count, visible and realized navigator-row counts, property-row count, horizontal and per-column scroll presence, overscan size, focused-target kind, popup-anchor presence, and reconciliation/rebuild counters. Diagnostics never include theme names, draft values, validation text, user-authored theme documents, or copied field text.

# Layout

The root fills the inline allocation supplied by the selected settings page and stacks the navigator above the property region. The navigator has a bounded block size. The property region grows with its external settings rows and remains part of the settings page's vertical flow.

The column trail is a single horizontal row whose intrinsic inline size follows the number of selected-path columns. Columns keep a stable width and do not shrink to avoid overflow. Each column fixes its header above a flexible vertical role-row viewport.

Role rows use a fixed block size and gap so visible-range calculation does not require measuring every schema row. Samples, labels, and disclosure affordances remain inside that fixed geometry. Long role labels truncate visually while their full accessible names remain available.

Spec CSS:

```css
.theme-editor {
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  min-inline-size: 0;
  inline-size: 100%;
  gap: var(--region-gap);
  color: var(--foreground);
}

.theme-editor__navigator {
  position: relative;
  min-inline-size: 0;
  block-size: var(--navigator-height);
  overflow: hidden;
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
}

.theme-editor__column-trail {
  display: flex;
  min-block-size: 0;
  block-size: 100%;
  gap: var(--column-gap);
  padding: var(--padding);
  overflow-x: auto;
  overflow-y: hidden;
}

.theme-editor__column {
  display: flex;
  flex: 0 0 var(--column-width);
  flex-direction: column;
  min-block-size: 0;
  border: var(--border-width) solid var(--border-color);
  background: var(--background);
}

.theme-editor__column-header {
  flex: none;
  block-size: var(--header-height);
  padding-inline: var(--padding-x);
  color: var(--foreground);
}

.theme-editor__role-viewport {
  position: relative;
  flex: 1 1 auto;
  min-block-size: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

.theme-editor__role-row {
  box-sizing: border-box;
  block-size: var(--row-height);
  margin-block-end: var(--row-gap);
  padding-inline: var(--padding-x);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
}

.theme-editor__role-row[data-state~="selected"] {
  background: var(--background);
  color: var(--foreground);
}

.theme-editor__properties {
  display: flex;
  flex-direction: column;
  min-inline-size: 0;
  gap: var(--row-gap);
}
```

# Variants

Default variant: settings-subpage editor with the role navigator above selected-role property rows.

No alternative navigation hierarchy or editor-workflow variants are defined.

# UI Roles

```css
.theme-editor {
  --region-gap: 12px;
  --foreground: #e7e3d8;
}

.theme-editor__navigator {
  --navigator-height: 156px;
  --border-width: 1px;
  --border-color: #31363b;
  --radius: 6px;
  --background: #111214;
}

.theme-editor__column-trail {
  --column-gap: 8px;
  --padding: 8px;
}

.theme-editor__column {
  --column-width: 176px;
  --border-width: 1px;
  --border-color: #31363b;
  --background: #1d2125;
}

.theme-editor__column-header {
  --header-height: 28px;
  --padding-x: 8px;
  --foreground: #8d959c;
}

.theme-editor__role-row {
  --row-height: 34px;
  --row-gap: 4px;
  --padding-x: 8px;
  --radius: 4px;
  --background: transparent;
  --foreground: #e7e3d8;
}

.theme-editor__role-row[data-state~="selected"] {
  --background: #21485d;
  --foreground: #f3f7f4;
}

.theme-editor__properties {
  --row-gap: 8px;
}
```
