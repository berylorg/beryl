# Name

Canonical name: discussion context

Sometimes known as: discussion-context panel, branch context panel

# Purpose

Presents readonly branch-discussion source context, provenance, resolution state, disclosure, and retry placement outside the conversation transcript.

# References

Contracts:

- disabled-command-tooltip
- scroll-ownership

Widgets:

- command button
- scrollbar
- tooltip

# Anatomy

The discussion context contains a bounded root panel, a fixed header, a disclosure command, a header label, an optional trailing state label, an optional retry command, a collapsed preview, an expanded readonly text viewport, compact provenance, and an external vertical scrollbar.

The owning feature supplies the exact context text, source provenance, stable discussion identity, state label, retry availability, command effects, and unavailable-context explanation. The widget owns disclosure mechanics, header/body geometry, readonly selection presentation, bounded scrolling, state treatment, and retry-command placement.

The context body is not a transcript record and does not host transcript actions, Quote, Resolve, Archive, or editable controls.

# Look

The panel reads as a compact contextual region between navigation chrome and the transcript. Its header stays present in collapsed and expanded states, so disclosure and resolution state do not move to another surface.

The header label is subdued and structural. Resolution state remains trailing and visually distinct without changing header height. The expanded context uses ordinary readable readonly text; provenance uses quieter emphasis.

The collapsed preview is one line and truncates without rewriting the underlying text. Compact provenance may truncate visually and expose its complete owner-supplied accessible label through a tooltip. Missing provenance or context uses an explicit unavailable treatment rather than substitute content.

# States

The widget supports expanded, collapsed, context available, context unavailable, resolution idle, resolution pending, handing off, handoff failed, archived, retry available, retry pending, body overflow, body scrolled, text selection active, focused disclosure, focused retry, and inert states.

State labels and their product meaning are owner supplied. Switching resolution state does not replace the panel or alter its allocated expanded height.

# Interaction

Activating the disclosure command toggles expanded and collapsed presentation. Enter and Space activate it when focused. Its accessible name reports the action that activation will perform.

Expanded text supports ordinary pointer and keyboard selection and copy. It is never editable and does not receive transcript context menus. The expanded body owns vertical scrolling while it has overflow; boundary propagation follows `scroll-ownership`.

Collapsing preserves the expanded body's scroll offset for the same stable discussion identity. Any selection hidden by collapse is cleared intentionally rather than retained against invisible geometry. Re-expansion restores the prior scroll offset when the context revision is unchanged.

The collapsed preview exposes the complete owner-supplied context through accessibility output. The visible line remains a truncation of the exact stored context and does not attempt to place the complete context in a tooltip.

A tooltip for truncated provenance remains anchored only while that provenance region is mounted. Collapsing preserves the header anchor; removing or replacing the stable discussion closes the tooltip intentionally.

Activating the retry command reports the exact stable discussion and admitted-job identity supplied by the owner. The widget does not resolve, archive, replace, or modify that job. While retry is pending, the command remains visible and disabled; duplicate pointer, keyboard, or programmatic activation is rejected.

Visible disabled commands satisfy `disabled-command-tooltip`. If a state update removes a focused retry command, focus moves to the disclosure command. If an update removes the panel, focus returns to the owner-supplied safe main-window target.

The context body is one bounded text record rather than a repeated collection. Its accepted UTF-8 byte ceiling is supplied and enforced by the branch-discussion feature before mount, so the widget does not realize unbounded source content or create a virtual row model for text lines.

Content-free diagnostics expose widget instance id, an opaque nonreversible discussion diagnostic key, context revision, disclosure state, resolution-state family, allocated height, overflow presence, scroll offset, selection presence, retry-control presence, focused-control kind, and tooltip-anchor presence. Diagnostics never include selected text, provenance labels, source titles, raw durable ids, error detail, resolution payloads, or tooltip text.

# Layout

The root fills the inline allocation supplied by `main-window.discussion-context`. Its expanded block size is content-derived up to a fixed owner-supplied maximum; its collapsed size is exactly the header plus one preview line.

The header is one fixed-height row. The structural label and disclosure command stay leading, optional state and retry controls stay trailing, and no header state wraps. The state label may truncate before a command is displaced.

The expanded body stacks the bounded readonly text viewport above compact provenance. The scrollbar overlays the text viewport's trailing edge. The panel never creates outer-window scrolling.

Spec CSS:

```css
.discussion-context {
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  min-inline-size: 0;
  inline-size: 100%;
  max-block-size: var(--max-height);
  border-block-end: var(--border-width) solid var(--border-color);
  background: var(--background);
  color: var(--foreground);
}

.discussion-context__header {
  display: flex;
  flex: none;
  align-items: center;
  min-inline-size: 0;
  block-size: var(--header-height);
  padding-inline: var(--padding-x);
  gap: var(--gap);
  color: var(--foreground);
}

.discussion-context__state {
  min-inline-size: 0;
  margin-inline-start: auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
}

.discussion-context__body {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-block-size: 0;
  padding-inline: var(--padding-x);
  padding-block-end: var(--padding-y);
  gap: var(--body-gap);
}

.discussion-context__text-viewport {
  position: relative;
  min-block-size: 0;
  max-block-size: var(--text-max-height);
  overflow: hidden;
  color: var(--foreground);
}

.discussion-context__preview {
  min-inline-size: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--foreground);
}

.discussion-context__provenance {
  min-inline-size: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--foreground);
  font-size: var(--font-size);
}

.discussion-context[data-state~="unavailable"],
.discussion-context[data-state~="inert"] {
  opacity: var(--opacity);
}
```

# Variants

Expanded and collapsed.

Default variant: expanded.

# UI Roles

```css
.discussion-context {
  --max-height: 260px;
  --border-width: 1px;
  --border-color: #334155;
  --background: #111827;
  --foreground: #e5e7eb;
}

.discussion-context__header {
  --header-height: 36px;
  --padding-x: 12px;
  --gap: 8px;
  --foreground: #94a3b8;
}

.discussion-context__body {
  --padding-x: 12px;
  --padding-y: 10px;
  --body-gap: 6px;
}

.discussion-context__text-viewport {
  --text-max-height: 184px;
  --foreground: #e5e7eb;
}

.discussion-context__preview {
  --foreground: #cbd5e1;
}

.discussion-context__provenance {
  --foreground: #94a3b8;
  --font-size: 11px;
}

.discussion-context__state {
  --foreground: #7dd3fc;
  --font-size: 11px;
  --font-weight: 600;
}

.discussion-context[data-state~="resolution-pending"] .discussion-context__state,
.discussion-context[data-state~="handing-off"] .discussion-context__state {
  --foreground: #7dd3fc;
}

.discussion-context[data-state~="handoff-failed"] .discussion-context__state {
  --foreground: #fca5a5;
}

.discussion-context[data-state~="archived"] .discussion-context__state {
  --foreground: #94a3b8;
}

.discussion-context[data-state~="unavailable"],
.discussion-context[data-state~="inert"] {
  --opacity: 0.62;
}
```
