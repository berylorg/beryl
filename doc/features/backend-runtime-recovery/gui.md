# Backend Runtime Recovery GUI

This is a normative supplemental GUI composition file for `design.md`. It owns placement and widget composition for backend-unavailable recovery in main conversation windows. User-visible recovery behavior, command availability, preserved state, and exact backend binding remain in `design.md`.

## Backend-Unavailable Notice Contribution

This feature mounts no `main-window notice`. When `design.md` makes a backend-unavailable notice
eligible, the feature supplies one owner-configured record to the Notifications per-window arbiter.
The record uses a stable selected-thread/runtime identity, bounded owner title and detail, and the
error and persistent variants.

The owner-supplied command region contains a `command button` labeled `Retry`. The persistent variant
omits the close command.

The feature maps Retry's design-owned enabled or disabled state, closest disabled explanation, and
pending state to that control. Retry progress and later record revisions reuse the same stable
notice identity. Notifications owns priority, persistence, replacement, and the sole visible notice
instance; this contribution adds no modal backdrop, transcript replacement, or conversation-body
layout space.

## Native-Lineage Recovery Prompt

Mount-into: main-window.user-input-panel

When execution reaches recovery-decision-required, this feature mounts one project-local `native
lineage recovery prompt` in place of the ordinary composer. The prompt receives a concise
owner-supplied explanation and exactly two `command button` controls labeled `Retry` and `Recover
from Syndic history`.

Before publishing the prompt, the outgoing composer host fences new editor mutations, settles any
active composition or pre-commit edit through the ordinary exact edit boundary, waits for every
already-admitted range-backed edit to reach its exact host terminal, incorporates each terminal
result into the current authoritative draft binding and revision, and captures the external
`text-input`'s exact compact restoration seed. Publishing then coherently unmounts the
ordinary `conversation composer` and its range-backed `text-input`; it does not keep either widget
hidden. Unmount cancels and releases the text-input's widget-owned requests, resident ranges,
staged local capacity, and other local resources under the external contract. The prompt receives
no draft content, editor source, or editor buffer and owns no composer restoration state.

`Retry` is the default and initially focused command. `Recover from Syndic history` remains visible
when unavailable and uses the expected-action-availability contract to present the exact
design-owned disabled reason. The feature maps each command's exact identity, enabled or disabled
state, closest explanation, pending state, and result; the canonical prompt owns only shared
presentation, focus, and layout.

The prompt uses the existing pinned user-input-panel allocation. It adds no backdrop, overlay,
dialog, persistent body row, or transcript replacement. Leaving the prompt does not reattach a
hidden editor. When restoration is eligible, the composer feature mounts a range-backed editor from
its compact host-owned restoration facts and bounded range requests, then returns focus to that
coherent editor; otherwise focus moves to the exact still-valid pending-turn control or thread
selector chosen by the owning feature.
