# Name

Canonical name: transcript view

Sometimes known as: transcript viewport, conversation transcript

# Purpose

Presents one coherent, resident conversation narrative and its explicitly supplied contextual records through a bounded realized frame without requiring all history or total transcript height to be loaded.

# References

Contracts:

- scroll-ownership

Widgets:

- anchored context menu
- code panel
- context menu
- image marker
- table panel

# Anatomy

The transcript view consists of a root surface, viewport, realized frame, ordered presentation records, optional synthetic-context groups, optional nested-widget hosts, selection layer, transient affordance layer, and stable local fallback records.

Each realized presentation record has an owner-supplied stable identity, revision, provenance classification, measured outer geometry, and authored-content, synthetic-context, or local-presentation classification. Authored content records may contain text runs, image markers, code panels, table panels, or other owner-supplied bounded nested widgets.

A synthetic-context group has an owner-supplied stable semantic identity and insertion boundary, structural heading, one or more bounded readonly text chunks, optional compact provenance, and an unavailable state. Multiple chunks share the group identity and render as one contextual item without becoming a second viewport or independently scrolling panel.

The transient affordance layer holds selection actions and menu anchors only while their exact realized geometry remains valid. Menus themselves use the referenced built-in widgets in the owning window overlay.

The widget consumes a host-supplied resident presentation snapshot and reports viewport, measurement, selection, menu-anchor, and nested-resource demand facts. It does not load, retain, evict, flatten, parse, or persist conversation history.

# Look

The transcript view reads as one continuous conversation surface rather than a stack of loading pages. Content records preserve their owner-supplied narrative hierarchy and Markdown presentation.

Synthetic-context groups remain visually distinct from authored turns while participating in the same vertical flow. Their heading, quoted body, and provenance read as one contextual callout rather than a message bubble or status panel.

Local fallback records are visually distinct from authored conversation content. Activation-pending treatment keeps the previous coherent frame visible but dimmed and inert until its replacement is published atomically.

The transcript viewport has no visual scrollbar chrome. Nested code and table panels retain their own bounded panel treatment and scrollbar affordances.

# States

The widget supports coherent, empty narrative, activation-pending, active, inert, tail-following, manually detached, leading-edge clamped, trailing-edge clamped, remeasuring, fallback-present, selection-active, selection-unavailable, menu-anchored, and nested-scroll-routed states.

Presentation records support authored content, synthetic context, local fallback, live, incomplete, context unavailable, resource pending, resource unavailable, and remeasurement pending states supplied by the owning feature and host.

Missing, pending, stale, rejected, or loading data never becomes a selectable content-record state.

# Interaction

Wheel, touchpad, keyboard, and programmatic manual scrolling apply exact pixel displacement. Manual scroll intent detaches automatic tail placement before demand resulting from that input is evaluated. Reaching a coherent resident edge clamps movement until the host supplies adjacent coherent records or a stable terminal fallback.

Selection begins and extends only through realized authored-content or synthetic-context records with stable geometry and owner-supplied selectable provenance. Transcript-level selection does not cross unrealized chunks. Copy and affordance requests report exact selected presentation identities, revisions, ranges, classification, and geometry to the owning feature.

Synthetic-context records may be selectable and copyable while remaining ineligible for owner-supplied quote, branch, edit, or turn-menu commands. Pointer or keyboard invocation creates no command target when the host marks that contextual classification non-actionable.

If activation, release, remeasurement, revision replacement, or virtualization invalidates selection or an affordance anchor, the widget closes that selection or affordance intentionally. It does not retain unbounded offscreen records solely to preserve an anchor.

Pointer and keyboard invocation over an eligible realized target reports its stable target identity and geometry. The owning feature supplies menu rows and command availability through the built-in context-menu and anchored-context-menu widgets.

Nested scroll routing follows `scroll-ownership` and the nested widget's own interaction contract. One pointer-wheel gesture has exactly one routed vertical owner and never co-scrolls the nested panel and transcript.

While activation is pending, the retained coherent frame does not accept selection, menu, media, or navigation interaction. Publishing the replacement frame and initial viewport state is one visible transition.

The widget realizes only records and synthetic-context chunks present in the current host-supplied realized-frame snapshot, including its bounded overscan. Rendering, hit testing, measurement, and accessibility construction never walk nonresident history, unreconciled context chunks, or elements outside that frame.

Stable presentation identity and synthetic-context group/chunk identity, not visible index, own measurement, selection, focus, nested-widget state, and menu anchoring across frame changes. A revision change invalidates measurements and facts from earlier revisions.

Content-free diagnostics expose widget instance id, transcript-view identity, activation revision, presentation revision, realized stable-id range, visible stable-id range, realized record count, measured record count, synthetic-context group count, realized context-chunk count, current anchor identity and viewport position, scroll mode, clamp direction, active pin counts, selection presence, menu-anchor presence, nested-widget counts, and fallback count. Diagnostics never include transcript text, discussion context, provenance labels, media labels, file paths, copied ranges, or menu labels.

# Layout

The root and viewport fill the inline and block allocation supplied by the conversation-body integration. The viewport clips nonrealized content and owns the transcript's vertical scroll position without assuming a continuous pixel extent for nonresident history.

The realized frame is positioned from the host-supplied semantic anchor and measured viewport position. Frame extension lays out adjacent realized records in narrative order. Anchor rebasing uses another already realized stable record while preserving that record's viewport position.

Presentation records occupy the available inline size, may contain variable-height content, and retain stable outer geometry while a nested widget scrolls internally. Nested widgets receive a bounded inline and block allocation from their presentation record.

A synthetic-context group occupies one semantic position at its host-supplied insertion boundary. Its realized chunks stack inside one visual group with bounded chunk spacing; unrealized chunks retain host-supplied virtual extent. The group has no inner scroll container, so ordinary transcript scrolling carries the complete item through the viewport.

Remeasurement after width, font, theme, content, resource, fallback, or nested-widget changes preserves the active semantic anchor or detached manual position through stable realized identities and current measurements.

The selection and transient-affordance layers overlay the realized frame without changing record measurement or scroll geometry. Activation-pending dimming likewise does not alter layout.

Spec CSS:

```css
.transcript-view {
  position: relative;
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  min-inline-size: 0;
  min-block-size: 0;
  inline-size: 100%;
  block-size: 100%;
  background: var(--background);
  color: var(--foreground);
}

.transcript-view__viewport {
  position: relative;
  min-inline-size: 0;
  min-block-size: 0;
  inline-size: 100%;
  block-size: 100%;
  overflow: hidden;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
}

.transcript-view__frame {
  position: relative;
  display: flex;
  flex-direction: column;
  min-inline-size: 0;
  gap: var(--record-gap);
}

.transcript-view__record {
  min-inline-size: 0;
  inline-size: 100%;
}

.transcript-view__record[data-kind~="fallback"] {
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
}

.transcript-view__context-group {
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  min-inline-size: 0;
  inline-size: 100%;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  gap: var(--chunk-gap);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
}

.transcript-view__context-heading {
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
  letter-spacing: var(--letter-spacing);
}

.transcript-view__context-body {
  min-inline-size: 0;
  color: var(--foreground);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.transcript-view__context-provenance {
  min-inline-size: 0;
  overflow: hidden;
  color: var(--foreground);
  font-size: var(--font-size);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.transcript-view__context-group[data-state~="unavailable"] {
  opacity: var(--opacity);
}

.transcript-view__selection-layer,
.transcript-view__affordance-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.transcript-view[data-state~="activation-pending"] {
  opacity: var(--opacity);
}
```

# Variants

Interactive narrative and readonly narrative.

The readonly narrative variant preserves scrolling and owner-supplied media inspection but does not create transcript selection actions or mutation-command targets.

Default variant: interactive narrative.

# UI Roles

```css
.transcript-view {
  --background: transparent;
  --foreground: #e5e7eb;
}

.transcript-view__viewport {
  --padding-x: 20px;
  --padding-y: 16px;
}

.transcript-view__frame {
  --record-gap: 14px;
}

.transcript-view__record[data-kind~="fallback"] {
  --padding-x: 12px;
  --padding-y: 10px;
  --border-width: 1px;
  --border-color: #475569;
  --radius: 6px;
  --background: #111827;
  --foreground: #94a3b8;
}

.transcript-view__context-group {
  --padding-x: 14px;
  --padding-y: 12px;
  --chunk-gap: 8px;
  --border-width: 1px;
  --border-color: #475569;
  --radius: 8px;
  --background: #111827;
  --foreground: #e2e8f0;
}

.transcript-view__context-heading {
  --foreground: #7dd3fc;
  --font-size: 10px;
  --font-weight: 700;
  --letter-spacing: 0.8px;
}

.transcript-view__context-body {
  --foreground: #e2e8f0;
}

.transcript-view__context-provenance {
  --foreground: #94a3b8;
  --font-size: 11px;
}

.transcript-view__context-group[data-state~="unavailable"] {
  --opacity: 0.62;
}

.transcript-view[data-state~="activation-pending"] {
  --opacity: 0.55;
}
```
