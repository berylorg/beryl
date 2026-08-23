# Name

Canonical name: conversation composer

Sometimes known as: composer panel, message composer

# Purpose

Provides the reusable conversation-input panel around a range-backed multiline text-input,
including adaptive height, inline atom presentation, bounded editor residency, writable
submission-disabled treatment, and fully inert treatment.

# References

Contracts:

- scroll-ownership

Widgets:

- image marker
- text-input

# Anatomy

The conversation composer consists of a root panel, editor surface, range-backed multiline
text-input, inline-atom hosts, bounded unrealized-region filler, and optional transient state
treatment.

The external text-input's range-backed variant owns text-editing mechanics, caret, compact logical
selection, bounded clipboard primitives, IME composition, opaque atom ranges, resident-range
wrapping, visible byte-range demand, inner vertical scrolling, routing ordinary edits through its
cursor-based bounded-page mutation protocol, and routing undo and redo through its fixed-size
historical-root selection protocol. The host supplies the authoritative revision-bound document
source and ordinary-edit sink plus opaque durable history authority and exact undo and redo
availability bound to the current editor binding, revision, and logical extent. The conversation
composer neither stores nor advances that frontier. The text-input retains only the visible range,
bounded overscan, active editing/IME ranges, one bounded pending ordinary-mutation session with
compact cursors, one fixed-size pending historical-selection intent, and compact range identities;
it never requests or stores the complete document, ordinary operation, inverse value, history
collection, or root graph.

The conversation composer owns the surrounding panel surface, adaptive panel measurement, bounded
filler and capacity-saturated state treatment, and integration of owner-supplied inline atom
widgets.

The widget does not contain a persistent submission button or a manual resize handle.

The owning feature supplies the current input model, atom identity and payload, placeholder, submission command, key-command mapping, persistence, autosave, history, validation, and domain meaning of every writable or inert state.

# Look

The widget reads as a pinned input surface whose editor remains visually stable as submission availability changes. Its border and background distinguish the writable area from the transcript without making it look like a modal form.

Submission-disabled treatment preserves normal readable text, markers, caret, and selection. Inert
treatment dims the complete panel and removes editing affordances while leaving its last coherent
content visible. Paste-pending uses the same stable dimmed surface without inserting progress text or
partial clipboard content.

Inline atoms remain compact and baseline-aligned with surrounding text. The active theme may style them through the shared image-marker roles rather than a composer-specific duplicate.

# States

The widget supports writable, focused, unfocused, empty, populated, submission-ready,
submission-disabled, inert, activation-pending, paste-pending, atom-present, growing, clamped,
inner-overflowing, inner-scrolling, historical-root-selection-pending, historical-root-rebind-
pending, capacity-saturated, and viewport-exceeds-rendering-capacity states.

Capacity saturation retains the coherent editor, caret, selection, active interaction anchor,
logical scroll extent, and undo/redo availability while bounded filler covers unrealized nominally
visible regions. It does not imply document truncation, edit rejection, or whole-viewport residency.

Submission-disabled and inert are distinct. Submission-disabled keeps the text-input editable; inert makes the text-input disabled or read-only according to the owner-supplied state and rejects draft-changing interaction.

Activation-pending retains the previously coherent composer until the owning feature publishes the replacement input and transcript selection atomically.

Paste-pending retains the coherent editor, caret, and selection while the owner streams one staged
paste. It suppresses draft-changing input and submission without replacing the editor or presenting
part of the incoming content.

# Interaction

Text editing, pointer selection, caret movement, clipboard behavior, IME, and inline-atom hit
testing follow the external text-input contract's range-backed multiline variant. Undo and redo
input is routed by text-input as a fixed-size historical-root selection intent containing the exact
current binding, revision, logical extent, opaque durable history-authority identity, direction,
operation identity, caret, and directed selection. It carries no inverse or replacement text,
marker registry, root graph, proposal page source, whole draft, or historical content. The host
decides exact availability, selects an existing immutable durable historical root, and advances or
preserves its opaque authority through one exact terminal result. Crossing a nonresident boundary
requests bounded document pages and preserves the last coherent editor frame until they arrive.

Pre-admission cancellation leaves the current root, caret, selection, and availability unchanged.
After admission, cancellation cannot override the result; the host reconciles indeterminate custody
until the operation settles exactly once as `Committed`, `Rejected`, `Conflict`, `Cancelled`, or
`Error`. The four noncommitted outcomes preserve the live editor and history frontier. `Committed`
supplies only the successor binding, revision, logical extent, exact caret and directed selection,
and successor authority identity with exact undo and redo availability. Every terminal settlement
is fixed-size control state and contains no draft, inverse or replacement payload, marker collection,
root graph, or source page. The text-input atomically retires the old live binding and installs those
compact successor facts, then realizes the selected root through its ordinary bounded text-page and
object-page sources. Any prior frame retained while that realization is pending is inert and cannot
accept editing or hit testing as successor content.

The owning feature chooses whether focused Enter propagates as a submission or edit-commit command and whether Shift+Enter inserts a newline. The widget reports those key events without defining acceptance, queueing, steering, or persistence effects.

When submission is disabled, all draft-editing interaction remains available and submission invocation reports the owner-supplied disabled outcome without clearing or replacing editor state.

When paste-pending, draft-changing keys, pointer edits, additional paste, undo, redo, and submission
are unavailable. `Escape` reports a cancellation request to the owner; the widget does not decide
whether the staged edit is still cancellable. Navigation or readonly selection remains available
only when the owner marks the retained range safe for that interaction.

When inert, pointer and keyboard input cannot mutate text or atoms. Existing content remains selectable only if the owning feature's inert reason explicitly permits readonly selection; otherwise the editor does not accept focus.

Inline image markers occupy indivisible opaque atom ranges in the text-input. The text-input owns
caret traversal and range selection and routes deletion, cut, paste, and other ordinary edits around
those atoms through the cursor-paged host-mutation protocol. Undo and redo instead select the
existing immutable root whose exact marker positions, caret, and directed selection are already
part of durable history authority. The host owns acceptance and authoritative coordination and
supplies the opaque undo/redo authority. Marker activation reports the exact stable atom identity
and geometry to the owning feature.

The composer host configures one finite settlement-custody capacity shared across live and detached
editor generations. Each ordinary commit or historical-root selection reserves one compact slot
before admission. Rebind or unmount transfers only the exact operation identity, base binding key,
and terminal reconciliation state into that slot; it transfers no source or proposal page, inverse
content, marker collection, root graph, whole draft, or widget instance. A later editor generation
may admit work only while another slot is available. Each late result matches and releases only its
reserved slot and cannot change the current editor generation, so repeated recovery, activation,
rebind, or unmount retains at most the configured number of fixed-size late-settlement records.
Custody exhaustion rejects a new operation before admission without changing exact durable
undo/redo availability or the current editor.

At a quiescent cut with no composition, ordinary mutation, historical-root selection, or unpublished
page or geometry work, the composer may pass through the text-input's payload-free compact
restoration seed. The seed binds the exact source binding, revision and logical extent; caret and
directed selection; logical scroll continuation; opaque history-authority identity; and exact undo
and redo availability. It contains no text, object or marker page, layout state, inverse or
replacement content, in-flight operation, detached-settlement custody, or widget instance. A new
mount validates that exact key and re-requests its bounded resident pages; it never revives an old
resident surface or operation.

When wrapped content exceeds the current panel clamp, the text-input owns vertical scrolling and keeps the caret or active selection endpoint visible. Scroll input propagates outward when the inner editor cannot scroll further, following `scroll-ownership`.

Panel growth or shrinkage remeasures surrounding layout without changing the editor document,
caret, selection, durable undo/redo availability, or inner scroll position except where the external
text-input must reveal the active endpoint.

Content-free diagnostics expose widget instance id, state family, focus presence, resident atom
count, resident visual line count, total logical bytes, resident text bytes, requested and visible
byte-range lengths, admitted editor-page count, measured content height, allocated panel height,
clamp presence, inner overflow presence, and text-input geometry revision. Diagnostics never include
draft text, atom labels, asset ids, clipboard content, or validation messages.

# Layout

The root panel fills the available inline size and uses content-derived block size between the owner-supplied minimum and maximum allocations. The owning feature configures the maximum allocation from the conversation layout, including its window-relative cap and required transcript minimum.

The editor surface fills the root panel. The range-backed multiline text-input wraps resident lines
to the available inline size and does not horizontally scroll. Before the clamp is reached, its
measured wrapped content grows or shrinks the panel. After the clamp is reached, panel height
remains bounded and the text-input owns vertical overflow. Total logical line count and total draft
size never determine resident layout or shaped-text storage.

Realization obeys owner-supplied retained-memory and per-frame work budgets rather than raw viewport
dimensions. Capacity goes first to caret, IME, and directed-selection geometry, then the active
interaction or scroll anchor, then nearby content. Any remaining nominally visible gap is
coalesced into bounded filler coverage; interaction re-anchors the external text-input and requests
only the next bounded work. The containing OS-window shell and renderer own rejection or clamping
of an unrepresentable drawable surface or framebuffer.

Inline atoms participate in text shaping as indivisible ranges. Their outer geometry contributes to line height without creating a separate panel row.

The transient state treatment overlays the root without changing its measured size. No state transition adds a banner, action row, or replacement placeholder to the widget.

`--owner-max-height` is a dynamic value supplied by the containing conversation layout after applying the feature-owned window and transcript constraints.

Spec CSS:

```css
.conversation-composer {
  position: relative;
  display: flex;
  box-sizing: border-box;
  min-inline-size: 0;
  inline-size: 100%;
  min-block-size: var(--min-height);
  max-block-size: var(--owner-max-height);
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
}

.conversation-composer__editor-surface {
  display: flex;
  min-inline-size: 0;
  min-block-size: 0;
  inline-size: 100%;
  block-size: 100%;
}

.conversation-composer__text-input {
  min-inline-size: 0;
  inline-size: 100%;
  block-size: 100%;
}

.conversation-composer[data-state~="focused"] {
  border-color: var(--border-color);
  box-shadow: 0 0 0 var(--ring-width) var(--ring-color);
}

.conversation-composer[data-state~="submission-disabled"] {
  border-color: var(--border-color);
}

.conversation-composer[data-state~="inert"],
.conversation-composer[data-state~="activation-pending"],
.conversation-composer[data-state~="paste-pending"] {
  opacity: var(--opacity);
}
```

# Variants

Ordinary input and replacement-edit input.

The variants share anatomy and editing mechanics. The owning feature supplies their key commands, state meaning, and visual annotations outside the reusable editor surface.

Default variant: ordinary input.

# UI Roles

```css
.conversation-composer {
  --owner-max-height: 320px;
  --min-height: 48px;
  --padding-x: 12px;
  --padding-y: 10px;
  --border-width: 1px;
  --border-color: #475569;
  --radius: 8px;
  --background: #111827;
  --foreground: #e5e7eb;
}

.conversation-composer[data-state~="focused"] {
  --border-color: #38bdf8;
  --ring-width: 1px;
  --ring-color: #38bdf8;
}

.conversation-composer[data-state~="submission-disabled"] {
  --border-color: #64748b;
}

.conversation-composer[data-state~="inert"],
.conversation-composer[data-state~="activation-pending"],
.conversation-composer[data-state~="paste-pending"] {
  --opacity: 0.55;
}
```
