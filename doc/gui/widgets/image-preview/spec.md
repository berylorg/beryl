# Name

Canonical name: image preview

Sometimes known as: image lightbox, fitted image popup

# Purpose

Shows one owner-supplied image in a bounded, transient window overlay while preserving aspect ratio, focus return, and local failure presentation.

# References

Widgets:

- command button

# Anatomy

The image preview consists of a backdrop, popup frame, image stage, fitted image surface, optional local state message, and close command.

The owner supplies preview identity, one admitted bounded thumbnail or visible tile set, local
failure state, accessibility label, and originating anchor. The widget does not locate, decode,
validate, persist, replace, mutate, or retain the complete original image bytes.

# Look

The preview reads as transient inspection chrome above the owning window. A subdued backdrop separates it from the underlying conversation while leaving the relationship to that window clear.

The image stage uses a neutral inset surface so transparent and unusually proportioned images remain legible. Loading, unavailable, unsupported, oversized, and decode-failed treatment stays local to the same stable popup frame.

The close command is always visible and does not overlap the fitted image.

# States

The widget supports closed, opening, open, focused, resource-pending, ready, unavailable, unsupported, oversized, decode-failed, and closing states.

Failure states preserve the popup frame, close command, focus contract, and owner-supplied image identity. They never substitute another resource silently.

# Interaction

Opening the preview records the exact originating marker or command and moves focus into the popup, initially to the close command when no other preview control exists.

Activating the close command, pressing Escape, or activating the backdrop closes the preview. Pointer activation inside the popup frame does not dismiss it.

Closure returns focus to the exact origin when that origin still exists and is eligible. Otherwise focus returns to the owning transcript or composer surface according to the feature's activation path.

The preview does not provide image editing, file replacement, drag export, external-viewer launch, zoom, pan, or submission commands.

If the owner replaces the resource revision while the preview is open, the widget accepts only a coherent owner-supplied replacement state. It does not retain stale decoded content under a new identity.

# Layout

The backdrop fills the owning OS-window overlay. The popup frame is centered and clamped inside that overlay with inset clearance from every edge.

The frame's maximum inline and block size derive from the available overlay size. The image stage consumes the remaining frame allocation after the close-command region. The image surface fits inside the stage with aspect ratio preserved and without enlargement beyond its owner-supplied intrinsic resolution.

The popup is non-user-resizable and has no internal scroll surface. Extremely large or unsupported resources use the owner-supplied bounded state instead of creating an unbounded image element.

State changes preserve the frame's constrained footprint so pending or failure treatment does not move the underlying layout.

Spec CSS:

```css
.image-preview {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  padding-inline: var(--inset-x);
  padding-block: var(--inset-y);
  background: var(--backdrop-background);
}

.image-preview__frame {
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  max-inline-size: min(var(--max-width), available-inline-size);
  max-block-size: min(var(--max-height), available-block-size);
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  gap: var(--gap);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  box-shadow: var(--shadow);
}

.image-preview__stage {
  display: flex;
  align-items: center;
  justify-content: center;
  min-inline-size: 0;
  min-block-size: 0;
  overflow: hidden;
  background: var(--background);
}

.image-preview__image {
  max-inline-size: 100%;
  max-block-size: 100%;
  object-fit: contain;
}

.image-preview__state-message {
  color: var(--foreground);
  font-size: var(--font-size);
}
```

# Variants

N/A

# UI Roles

```css
.image-preview {
  --inset-x: 36px;
  --inset-y: 32px;
  --backdrop-background: rgba(2, 6, 23, 0.72);
}

.image-preview__frame {
  --max-width: 1200px;
  --max-height: 900px;
  --padding-x: 14px;
  --padding-y: 14px;
  --gap: 10px;
  --border-width: 1px;
  --border-color: #475569;
  --radius: 10px;
  --background: #0f172a;
  --foreground: #e5e7eb;
  --shadow: 0 18px 48px rgba(0, 0, 0, 0.55);
}

.image-preview__stage {
  --background: #020617;
}

.image-preview__state-message {
  --foreground: #cbd5e1;
  --font-size: 13px;
}
```
