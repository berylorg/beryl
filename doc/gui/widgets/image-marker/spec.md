# Name

Canonical name: image marker

Sometimes known as: image atom, image label marker

# Purpose

Presents a compact, stable inline reference to an image without rendering a thumbnail or exposing storage identity.

# References

Widgets:

- text-input

# Anatomy

The image marker consists of an inline root, owner-supplied visible label, optional media-state
indicator, and stable activation anchor.

The owner supplies marker identity, label text, accessibility text, media-identity state,
presentation-availability state, and activation behavior. The widget does not own image bytes,
labels, asset references, commands, menus, previews, editing semantics, submission, or persistence.

# Look

The marker reads as a compact inline atom distinct from ordinary authored text. It preserves a
consistent capsule-like treatment across editable and readonly owners while allowing those variants
to communicate their interaction level.

Pending and unavailable treatment keeps the label legible and does not replace it with a generic
loading or broken-image icon. Hover, focus, pressed, selected, pending, and unavailable feedback
preserve the marker's inline geometry.

# States

The widget supports normal, hover, pressed, focused, selected, readonly, editable,
activation-pending, contextual-command-pending, contextual-command-open, pending, admitted,
unavailable, rendition-pending, ready, and local-unavailable states.

`pending`, `admitted`, and `unavailable` are mutually exclusive owner-supplied media-identity
states. An admitted marker may additionally be `rendition-pending`, `ready`, or
`local-unavailable`. These mappings render the owning feature's states but do not define when a
product changes state.

Media identity, local presentation availability, activation eligibility, and editability are
independent. An unavailable or locally unavailable marker may remain activatable so its owner can
present status or contextual actions.

Activation-pending represents a reported primary-activation request whose outcome has not yet been
reported by the owner. Contextual-command-pending represents a reported contextual-command request
that the owner has not settled. Contextual-command-open represents an attached contextual command
surface that is currently open for this marker.

# Interaction

Primary pointer activation or focused Enter or Space reports the exact stable marker identity,
current media states, and anchor geometry to the owner. The widget does not define the resulting
commands or their availability.

After reporting a primary activation, the marker enters activation-pending. Repeated primary
pointer, Enter, Space, or programmatic activation for the same stable marker identity is suppressed
while that state remains set. The owner clears activation-pending only by reporting that activation
opened its requested surface, completed without an open surface, was rejected, or was cancelled.

A context-menu gesture reports the same stable identity, media states, and geometry through a
distinct contextual-command request. The widget does not create or position the resulting command
surface.

After reporting that request, the marker enters contextual-command-pending and suppresses repeated
context-menu or programmatic contextual requests for the same stable marker identity. Opening the
requested surface clears contextual-command-pending and enters contextual-command-open. Rejection
or cancellation clears contextual-command-pending without opening a surface.

When the owner attaches a contextual command surface, every command state is owner-supplied to that
canonical surface. The marker never enables a command merely because it is visible or admitted.
Further contextual-command requests for the same stable marker identity are suppressed while the
surface remains contextual-command-open. The canonical surface owns its dismissal triggers.
Dismissal clears contextual-command-open, does not issue another contextual-command request, and
returns focus to the exact eligible marker, with fallback focus supplied by the owner. Disabled
contextual activation leaves focus on the command or marker origin.

Inside a text-input atom range, caret movement, range selection, deletion, clipboard behavior,
undo, and redo follow the referenced text-input variant and its host-authority boundary. Marker
activation does not silently mutate the containing text.

Inside readonly content, activation may open an owner-supplied inspection action. The marker itself
never opens an external viewer or resolves image bytes.

If the marker is removed, unrealized, or revision-replaced, the host cancels any pending primary or
contextual request for that identity and intentionally closes any owner-supplied popover, menu, or
preview overlay anchored to it. This clears activation-pending, contextual-command-pending, and
contextual-command-open. The widget does not remain realized solely as an offscreen anchor.

# Layout

The marker participates in surrounding inline layout as one indivisible range. Its label does not wrap internally, and the marker does not create a separate line unless ordinary line wrapping moves the whole atom.

The root aligns to the surrounding text baseline. State indicators fit inside the same measured bounds so state changes do not reflow the containing paragraph or editor line.

Spec CSS:

```css
.image-marker {
  display: inline-flex;
  align-items: center;
  box-sizing: border-box;
  max-inline-size: 100%;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
  line-height: var(--line-height);
  white-space: nowrap;
  vertical-align: baseline;
}

.image-marker[data-state~="hover"] {
  background: var(--background);
  border-color: var(--border-color);
}

.image-marker[data-state~="pressed"] {
  background: var(--background);
}

.image-marker[data-state~="focused"] {
  box-shadow: 0 0 0 var(--ring-width) var(--ring-color);
}

.image-marker[data-state~="selected"] {
  background: var(--background);
  color: var(--foreground);
}

.image-marker[data-state~="pending"] {
  color: var(--foreground);
  border-color: var(--border-color);
  opacity: var(--opacity);
}

.image-marker[data-state~="rendition-pending"] {
  color: var(--foreground);
  border-color: var(--border-color);
  opacity: var(--opacity);
}

.image-marker[data-state~="unavailable"] {
  color: var(--foreground);
  border-color: var(--border-color);
  opacity: var(--opacity);
}

.image-marker[data-state~="local-unavailable"] {
  color: var(--foreground);
  border-color: var(--border-color);
  opacity: var(--opacity);
}
```

# Variants

Editable atom and readonly inline marker.

The editable atom variant participates in containing editor atom semantics. The readonly inline
variant participates in its owner's selection model as one owner-supplied content span.

Default variant: readonly inline marker.

# UI Roles

```css
.image-marker {
  --padding-x: 5px;
  --padding-y: 1px;
  --border-width: 1px;
  --border-color: #3b82f6;
  --radius: 5px;
  --background: #172554;
  --foreground: #bfdbfe;
  --font-size: 12px;
  --font-weight: 600;
  --line-height: 16px;
}

.image-marker[data-state~="hover"] {
  --background: #1e3a8a;
  --border-color: #60a5fa;
}

.image-marker[data-state~="pressed"] {
  --background: #1e40af;
}

.image-marker[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #38bdf8;
}

.image-marker[data-state~="selected"] {
  --background: #2563eb;
  --foreground: #eff6ff;
}

.image-marker[data-state~="pending"] {
  --foreground: #bfdbfe;
  --border-color: #60a5fa;
  --opacity: 0.8;
}

.image-marker[data-state~="rendition-pending"] {
  --foreground: #bfdbfe;
  --border-color: #60a5fa;
  --opacity: 0.8;
}

.image-marker[data-state~="unavailable"] {
  --foreground: #94a3b8;
  --border-color: #64748b;
  --opacity: 0.8;
}

.image-marker[data-state~="local-unavailable"] {
  --foreground: #94a3b8;
  --border-color: #64748b;
  --opacity: 0.8;
}
```
