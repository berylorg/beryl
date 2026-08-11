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

The transcript view consists of a root surface, viewport, realized frame, ordered presentation records, optional record-status/provenance parts, optional synthetic-context groups, optional nested-widget hosts, selection layer, transient affordance layer, and stable local fallback-record parts. Every local fallback record uses both the base record part and the fallback-record part, so it retains ordinary record geometry while receiving fallback-specific styling.

Each realized presentation record has an owner-supplied stable identity, revision, provenance classification, measured outer geometry, and authored-content, synthetic-context, or local-presentation classification. Authored content records may contain text runs, image markers, code panels, table panels, or other owner-supplied bounded nested widgets.

An authored-content record may include one optional noninteractive record-status/provenance part. The
part carries an owner-supplied visible label, matching accessible label, and one pending, repaired,
or incomplete provenance state. It belongs to the same host generation and record identity as the
content it describes; it is not a separate presentation record, command target, or live region.

A synthetic-context group has an owner-supplied stable semantic identity and insertion boundary, structural heading, one or more bounded readonly text chunks, optional compact provenance, and an unavailable state. Multiple chunks share the group identity and render as one contextual item without becoming a second viewport or independently scrolling panel.

The transient affordance layer holds selection actions and menu anchors only while their exact realized geometry remains valid. Menus themselves use the referenced built-in widgets in the owning window overlay.

One live authored-content record may combine a host-supplied durable prefix with one bounded transient suffix carrying the same exact item identity and a later logical-text frontier. The suffix is local presentation state until the host replaces it from an exactly matching durable projection; it is not a second transcript record or an independently scrolling surface.

The widget consumes a host-supplied resident presentation snapshot whose content refers to immutable
bounded pages or bounded generations and reports viewport, measurement, selection, menu-anchor, and
nested-resource demand facts. Snapshot construction and widget reconciliation never deep-clone
text, resource bytes, nested models, or the resident collection. The widget does not load, retain,
evict, flatten, parse, or persist conversation history.

The widget exposes one focus-pin demand slot to its host. The slot is absent or carries one stable
presentation-record identity, presentation revision, transcript-view identity, owning
host/repair-publication generation identity, and, only while focus is inside a nested widget, that
widget's stable identity and revision. It never creates separate focus pins for nested controls,
chunks, or resource ranges.

# Look

The transcript view reads as one continuous conversation surface rather than a stack of loading pages. Content records preserve their owner-supplied narrative hierarchy and Markdown presentation.

Synthetic-context groups remain visually distinct from authored turns while participating in the same vertical flow. Their heading, quoted body, and provenance read as one contextual callout rather than a message bubble or status panel.

Local fallback records are visually distinct from authored conversation content. Activation-pending treatment keeps the previous coherent frame visible but dimmed and inert until its replacement is published atomically.

When present, the record-status/provenance part reads as compact supporting provenance attached to
its record. Pending, repaired, and incomplete treatments remain distinguishable without competing
with the authored content hierarchy or resembling an interactive badge.

The transcript viewport has no visual scrollbar chrome. Nested code and table panels retain their own bounded panel treatment and scrollbar affordances.

# States

The widget supports coherent, empty narrative, activation-pending, active, inert, tail-following,
manually detached, arrival-streaming, durable-reconciling, leading-edge clamped, trailing-edge
clamped, remeasuring, fallback-present, focus-outside, direct-record-focused, nested-widget-focused,
selection-active, selection-unavailable, menu-anchored, and nested-scroll-routed states.

Presentation records support authored content, synthetic context, local fallback, live, incomplete, context unavailable, resource pending, resource unavailable, and remeasurement pending states supplied by the owning feature and host.

The optional record-status/provenance part supports `present`, `pending`, `repaired`, and
`incomplete` states. `present` means the part participates in the current record; when present it has
exactly one of `pending`, `repaired`, or `incomplete`. Absence means the part is not realized and
does not reserve layout space.

Missing, pending, stale, rejected, or loading data never becomes a selectable content-record state.

# Interaction

Wheel, touchpad, keyboard, and programmatic manual scrolling apply exact pixel displacement. Manual scroll intent detaches automatic tail placement before demand resulting from that input is evaluated. Reaching a coherent resident edge clamps movement until the host supplies adjacent coherent records or a stable terminal fallback.

Selection begins and extends only through realized authored-content or synthetic-context records with stable geometry and owner-supplied selectable provenance. Transcript-level selection does not cross unrealized chunks. Copy and affordance requests report exact selected presentation identities, revisions, ranges, classification, and geometry to the owning feature.

Synthetic-context records may be selectable and copyable while remaining ineligible for owner-supplied quote, branch, edit, or turn-menu commands. Pointer or keyboard invocation creates no command target when the host marks that contextual classification non-actionable.

If activation, release, remeasurement, revision replacement, or virtualization invalidates selection or an affordance anchor, the widget closes that selection or affordance intentionally. It does not retain unbounded offscreen records solely to preserve an anchor.

Pointer and keyboard invocation over an eligible realized target reports its stable target identity and geometry. The owning feature supplies menu rows and command availability through the built-in context-menu and anchored-context-menu widgets.

The record-status/provenance part is static text. It never receives focus, accepts activation,
starts selection, opens a menu, or becomes a command target. Its owner-supplied accessible label is
included in the containing record's accessible description and exposes the same state and meaning
as the visible label.

The part's presence, label, and pending, repaired, or incomplete state update only when the widget
consumes a new owning host generation. The widget never transitions repair status from local input,
timers, live fragments, buffered facts, geometry changes, or paint state.

Nested scroll routing follows `scroll-ownership` and the nested widget's own interaction contract. One pointer-wheel gesture has exactly one routed vertical owner and never co-scrolls the nested panel and transcript.

When focus enters an eligible realized record, the widget reports the single focus-pin fact with
the current transcript-view identity, stable presentation-record identity, presentation revision,
and owning host/repair-publication generation identity. Focus inside a nested widget adds that
widget's stable identity and revision to the same fact. Moving focus to another transcript target
replaces the fact atomically; it never retains both origin and destination pins.

Owner-requested focus return to the transcript surface resolves to the exact eligible realized
record at the current semantic anchor and reports it as direct-record focus. If no such record
exists, the widget does not create an identity-free pin and the owning feature continues its
fallback chain.

The host accepts the fact only when every carried identity and revision matches the current
coherent snapshot. Rejection or later staleness releases the fact and asks the owning feature to
transfer focus through its documented fallback chain. The widget does not match replacement
content by visible index, text, label, widget type, or semantic-anchor proximity.

Focus transfer outside the transcript, a nested widget reporting blur or transfer away, removal or
replacement of focused content without exact identity continuity, selected-thread switch, owning
window close, and host disposal release the fact. A focused record may stay realized outside
ordinary overscan only while the accepted fact remains valid; the pin may retain only that record,
the minimum resource range needed by the focused target, and the minimum state of the exact nested
widget.

While activation is pending, the retained coherent frame does not accept selection, menu, media, or navigation interaction. Publishing the replacement frame and initial viewport state is one visible transition.

An arrival-streaming record exposes every newly supplied bounded normalized-text fragment on the
next frame that consumes it. The widget preserves parent-delta identity and order and does not
subdivide, delay, timestamp-replay, or animate a fragment character by character; multiple
fragments received before one frame may naturally publish together. Exact durable-prefix
reconciliation replaces the matching transient suffix without producing duplicate text or a blank
intermediate record.

The widget realizes only records and synthetic-context chunks present in the current host-supplied realized-frame snapshot, including its bounded overscan. Rendering, hit testing, measurement, and accessibility construction never walk nonresident history, unreconciled context chunks, or elements outside that frame.

Stable presentation identity and synthetic-context group/chunk identity, not visible index, own measurement, selection, focus, nested-widget state, and menu anchoring across frame changes. A revision change invalidates measurements and facts from earlier revisions.

Content-free diagnostics expose widget instance id, transcript-view identity, activation revision,
presentation revision, realized stable-id range, visible stable-id range, realized record count,
measured record count, synthetic-context group count, realized context-chunk count, current anchor
identity and viewport position, scroll mode, clamp direction, active pin counts, focus-pin presence,
focus-pin state (`absent`, `direct-record`, or `nested-widget`), focus-pin accept, reject, release,
and transfer counts, selection presence, menu-anchor presence, nested-widget counts, and fallback
count. Focus-pin diagnostics expose no focused target identity. Diagnostics never include transcript text,
discussion context, provenance labels, media labels, file paths, copied ranges, resource bytes,
nested-widget content, or menu labels.

# Layout

The root and viewport fill the inline and block allocation supplied by the conversation-body integration. The viewport clips nonrealized content and owns the transcript's vertical scroll position without assuming a continuous pixel extent for nonresident history.

The realized frame is positioned from the host-supplied semantic anchor and measured viewport position. Frame extension lays out adjacent realized records in narrative order. Anchor rebasing uses another already realized stable record while preserving that record's viewport position.

Presentation records occupy the available inline size, may contain variable-height content, and retain stable outer geometry while a nested widget scrolls internally. Nested widgets receive a bounded inline and block allocation from their presentation record.

When present, the record-status/provenance part follows the record's owner-supplied provenance in
the same metadata flow. It remains compact, truncates rather than forcing horizontal overflow, and
participates in the record's ordinary measured geometry. It never overlays authored content or
reserves space while absent.

A synthetic-context group occupies one semantic position at its host-supplied insertion boundary. Its realized chunks stack inside one visual group with bounded chunk spacing; unrealized chunks retain host-supplied virtual extent. The group has no inner scroll container, so ordinary transcript scrolling carries the complete item through the viewport.

Remeasurement after width, font, theme, content, resource, fallback, or nested-widget changes preserves the active semantic anchor or detached manual position through stable realized identities and current measurements.

Live-tail growth and durable reconciliation retain the same outer record identity. They may change its measured extent, but they do not insert a second row or reset the current semantic anchor solely because the authoritative frontier advanced.

The selection and transient-affordance layers overlay the realized frame without changing record measurement or scroll geometry. Activation-pending dimming likewise does not alter layout.

The one accepted focus pin may extend realization only enough to retain its exact record and
minimum nested-widget state. It never expands the visible range, overscan, virtual extent, or
resource realization to a whole turn, complete resource, or offscreen collection.

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

.transcript-view__record-status {
  display: inline-flex;
  box-sizing: border-box;
  max-inline-size: 100%;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  overflow: hidden;
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
  line-height: var(--line-height);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.transcript-view__record-status[data-state~="pending"],
.transcript-view__record-status[data-state~="repaired"],
.transcript-view__record-status[data-state~="incomplete"] {
  border-color: var(--border-color);
  background: var(--background);
  color: var(--foreground);
}

.transcript-view__fallback-record {
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

.transcript-view__record-status {
  --padding-x: 8px;
  --padding-y: 3px;
  --border-width: 1px;
  --border-color: #475569;
  --radius: 999px;
  --background: #111827;
  --foreground: #cbd5e1;
  --font-size: 11px;
  --font-weight: 600;
  --line-height: 16px;
}

.transcript-view__record-status[data-state~="pending"] {
  --border-color: #92400e;
  --background: #1c1917;
  --foreground: #fbbf24;
}

.transcript-view__record-status[data-state~="repaired"] {
  --border-color: #166534;
  --background: #052e16;
  --foreground: #86efac;
}

.transcript-view__record-status[data-state~="incomplete"] {
  --border-color: #991b1b;
  --background: #450a0a;
  --foreground: #fca5a5;
}

.transcript-view__fallback-record {
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
