# Name

Canonical name: image preview

Sometimes known as: image lightbox, fitted image popup

# Purpose

Shows one owner-supplied image in a bounded, transient modal overlay within the owning window while
preserving aspect ratio, focus return, and local failure presentation.

# References

Widgets:

- command button

# Anatomy

The image preview consists of a backdrop, popup frame, preview-chrome command region, image stage,
fitted image surface, optional local state message, optional contextual-command anchor, and close
command. The optional anchor and required close command use command buttons in the chrome region.

The owner supplies preview identity, media-identity state, one admitted bounded thumbnail or visible
tile set when ready, local presentation state, accessibility label, originating anchor, and any
contextual-command availability. The widget does not locate, decode, validate, persist, replace,
mutate, or retain the complete original image bytes and does not decide product command policy.

# Look

The preview reads as transient inspection chrome above the owning window. A subdued backdrop
separates it from underlying content while leaving the relationship to that window clear.

The image stage uses a neutral inset surface so transparent and unusually proportioned images remain legible. Loading, unavailable, unsupported, oversized, and decode-failed treatment stays local to the same stable popup frame.

The close command is always visible and does not overlap the fitted image. Owner-supplied
contextual-command anchoring does not obscure media or state text.

# States

The widget supports closed, opening, open, focused, closing, pending, admitted, unavailable,
rendition-pending, ready, local-unavailable, unsupported, oversized, and decode-failed states.

`pending`, `admitted`, and `unavailable` are mutually exclusive owner-supplied media-identity states.
An admitted preview may additionally be `rendition-pending`, `ready`, or `local-unavailable`;
unsupported, oversized, and decode-failed refine the local unavailable reason. These mappings
render owner-supplied state and do not define product transition policy.

Pending and failure states preserve the popup frame, close command, contextual-command anchor,
focus contract, and owner-supplied image identity. They never substitute another resource silently.

# Interaction

Opening the preview records the exact originating marker or command and moves focus into the popup,
initially to the close command.

While open, the preview is modal within the owning OS window. When no owner-supplied child command
surface or external flow owns focus, focus is contained within the popup frame: Tab and Shift+Tab
cycle through its eligible controls, and attempts to move focus to underlying window content are
redirected to the last focused eligible preview control or the close command. An active child
command surface owns its internal focus rules but remains inside the preview's modal scope.

Activating the contextual-command anchor reports the preview identity and current anchor geometry
to the owner. The widget does not define the commands, their availability, or the canonical command
surface composed at that anchor.

When the owner-supplied command surface closes, focus returns to the exact eligible
contextual-command anchor. If an invoked command opens a platform picker or another external flow,
closing that flow returns focus to the anchor when the preview still exists. If the preview has
closed meanwhile, focus follows the preview's origin-return contract.

Activating the close command, or pressing Escape while the preview owns Escape, closes the preview.
Pointer activation inside the popup frame does not dismiss it. Pointer activation on the backdrop
is consumed by the preview before closure; it closes the preview without propagating, retargeting,
or replaying that activation to underlying content.

Closure returns focus to the exact origin when that origin still exists and is eligible. Otherwise
focus returns to the owner-supplied stable fallback surface.

The preview does not provide image editing, file replacement, drag export, external-viewer launch, zoom, pan, or submission commands.

If the owner replaces the resource revision while the preview is open, the widget accepts only a coherent owner-supplied replacement state. It does not retain stale decoded content under a new identity.

# Layout

The backdrop fills the owning OS-window overlay. The popup frame is centered and clamped inside that overlay with inset clearance from every edge.

The frame's maximum inline and block size derive from the available overlay size. The image stage consumes the remaining frame allocation after the close-command region. The image surface fits inside the stage with aspect ratio preserved and without enlargement beyond its owner-supplied intrinsic resolution.

The preview-chrome command region keeps the close command available and separate from the image
stage. When present, the contextual-command anchor shares that region without overlapping the state
message or image. Its owner-supplied command surface is positioned by that canonical surface's own
anchoring contract.

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

.image-preview__chrome {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: var(--gap);
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
