# Name

Canonical name: native lineage recovery prompt

Sometimes known as: composer recovery prompt

# Purpose

Presents the exact Retry-or-recover decision when a conversation cannot reuse its native CAS
lineage, while occupying the ordinary user-input-panel position without becoming a modal dialog or
discarding the hidden composer state.

# References

Contracts:

- expected-action-availability

Widgets:

- command button

# Anatomy

The native lineage recovery prompt contains a root panel, message region, heading, bounded
explanation, and command group.

The command group contains exactly two owner-configured command buttons. The owning feature supplies
their labels, stable command identities, availability, loading state, disabled explanations, and
effects. The prompt owns their shared arrangement and focus movement but does not decide whether a
binding may be retried or retired.

The widget contains no text editor, draft copy, transcript content, error-detail scroller, close
command, or dismissal affordance. The owning feature retains the ordinary composer and its state
outside this widget while the prompt is mounted.

# Look

The prompt reads as a compact inline replacement for the pinned composer. It uses the same general
panel material and boundary relationship as the composer without imitating an editable field.

The heading identifies that continuation needs attention. The explanation is visually secondary
and the command group remains immediately legible. Retry receives the default-command treatment;
recovery remains visually distinct without using destructive styling because it does not delete
Syndic history.

The prompt has no backdrop, floating shadow, dialog title bar, or overlay treatment.

# States

The widget supports ready, retry-running, recovery-running, retry-disabled, recovery-disabled,
focused-command, and leaving states.

Exactly one running state may be present. While a command is running, both commands reject duplicate
activation. A disabled command remains visible and satisfies `expected-action-availability`.

Leaving retains the current presentation until the owning feature publishes either the restored
composer or a successor pending-turn state; it never briefly exposes an empty user-input panel.

# Interaction

On initial entry, focus moves to Retry unless the owning feature identifies a still-valid focused
control that must finish its current key event before focus transfer. Tab and Shift+Tab move between
the enabled command buttons without leaving an unreachable focus target inside the prompt.

Enter and Space invoke the focused command according to the `command button` contract. Escape does
not dismiss the prompt, restore the composer, change the selected thread, or abandon the retained
binding.

When both commands are temporarily unavailable, the prompt itself may receive programmatic focus
for accessible reading, but it performs no command. Disabled buttons expose their owner-supplied
explanations through the referenced availability contract.

On replacement by an eligible composer, focus returns to the restored editor. If restoration is
not eligible because an already-admitted turn owns the input, the owning feature supplies the exact
safe focus destination outside this widget.

# Layout

The root fills the user-input panel's available inline size and uses compact content-derived block
size. It does not preserve a previously tall draft-driven composer allocation; the surrounding
conversation layout returns the released height to the transcript region.

The message region consumes remaining inline space before the command group. Explanation text wraps
within its region and cannot push either command outside the available panel width.

The command group keeps both controls adjacent in ordinary space. Under constrained width it wraps
as a unit below the message region rather than truncating either label or introducing horizontal
scrolling. The widget never owns vertical scrolling.

Spec CSS:

```css
.native-lineage-recovery-prompt {
  display: flex;
  flex-wrap: wrap;
  box-sizing: border-box;
  align-items: center;
  min-inline-size: 0;
  inline-size: 100%;
  min-block-size: var(--min-height);
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  gap: var(--gap);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
}

.native-lineage-recovery-prompt__message {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-inline-size: 0;
  gap: var(--gap);
}

.native-lineage-recovery-prompt__heading {
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
}

.native-lineage-recovery-prompt__explanation {
  color: var(--foreground);
  font-size: var(--font-size);
}

.native-lineage-recovery-prompt__commands {
  display: flex;
  flex: 0 1 auto;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: var(--gap);
}

.native-lineage-recovery-prompt[data-state~="leaving"] {
  opacity: var(--opacity);
}
```

# Variants

N/A.

# UI Roles

```css
.native-lineage-recovery-prompt {
  --min-height: 64px;
  --padding-x: 12px;
  --padding-y: 10px;
  --gap: 12px;
  --border-width: 1px;
  --border-color: #f59e0b;
  --radius: 8px;
  --background: #111827;
  --foreground: #e5e7eb;
}

.native-lineage-recovery-prompt__message {
  --gap: 4px;
}

.native-lineage-recovery-prompt__heading {
  --foreground: #f8fafc;
  --font-size: 13px;
  --font-weight: 600;
}

.native-lineage-recovery-prompt__explanation {
  --foreground: #cbd5e1;
  --font-size: 12px;
}

.native-lineage-recovery-prompt__commands {
  --gap: 8px;
}

.native-lineage-recovery-prompt[data-state~="leaving"] {
  --opacity: 0.7;
}
```
