# Name

Canonical name: image marker

Sometimes known as: image atom, image label marker

# Purpose

Presents a compact, stable inline reference to an image without rendering a thumbnail or exposing storage identity.

# References

N/A

# Anatomy

The image marker consists of an inline root, owner-supplied visible label, optional unavailable indicator, and stable activation anchor.

The owner supplies marker identity, label text, accessibility text, availability meaning, and activation behavior. The widget does not own image bytes, labels, asset references, menus, previews, editing semantics, submission, or persistence.

# Look

The marker reads as a compact inline atom distinct from ordinary authored text. It preserves a consistent capsule-like treatment in composer and transcript contexts while allowing readonly and editable variants to communicate their interaction level.

Unavailable treatment keeps the label legible and does not replace it with a generic broken-image icon. Hover, focus, pressed, selected, and unavailable feedback preserve the marker's inline geometry.

# States

The widget supports normal, hover, pressed, focused, selected, readonly, editable, unavailable, and activation-pending states.

Availability and editability are independent. An unavailable marker can remain activatable so its owning feature may explain or retry the unavailable resource.

# Interaction

Primary pointer activation or focused Enter or Space reports the exact stable marker identity and current anchor geometry to the owning feature. The widget does not define the resulting commands.

Inside a text-input atom range, caret movement, range selection, deletion, clipboard behavior, undo, and redo remain owned by the text-input and composer. Marker activation does not silently mutate the containing text.

Inside readonly transcript content, activation may open an owner-supplied inspection action. The marker itself never opens an external viewer or resolves image bytes.

If the marker is removed, unrealized, or revision-replaced while an owner-supplied popup is anchored to it, the host closes that popup intentionally. The widget does not remain realized solely as an offscreen anchor.

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

.image-marker[data-state~="unavailable"] {
  color: var(--foreground);
  border-color: var(--border-color);
  opacity: var(--opacity);
}
```

# Variants

Editable atom and readonly inline marker.

The editable atom variant participates in containing editor atom semantics. The readonly inline variant participates in transcript selection as one owner-supplied content span.

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

.image-marker[data-state~="unavailable"] {
  --foreground: #94a3b8;
  --border-color: #64748b;
  --opacity: 0.8;
}
```
