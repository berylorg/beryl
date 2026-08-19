# Composer Whole-Operation And Inverse-History Assumption

## Invalidated Approach

The planned large-composer undo path expected `gpui-text-input` to retain every fragment of one
logical mutation behind a finite cumulative ceiling, then expected the Beryl host to stream copied
inverse text and marker witnesses from historical roots through ordinary multi-edit reconstruction.

## Why It Failed

That combination makes logical edit and undo size depend on a whole-operation resident collection
and an arbitrary fragment count. It also loses the clean identity of the historical immutable root,
cannot place restored inline objects at successor-relative positions in newly inserted text, and
turns one undo into reconstruction work whose partial progress, cancellation, replay, collision,
and exact directed-selection result cannot share one terminal settlement.

Viewport-proportional realization was the same category of mistake: a nominal drawable area can
exceed the configured retained projection even though the logical document and paged scroll extent
remain valid.

## Course Correction

Use one app-neutral cursor/session protocol with bounded source and proposal pages, canonical
cumulative identity, explicit finish-input, immediate payload release, and one terminal settlement.
Small edits use its one-page fast path; large edits make bounded progress without a cumulative
fragment cap.

Store compact durable same-draft root-transition journal/frontier records in Syndic/Fjall. Ordinary
candidate adoption appends a transition; undo and redo directly adopt an authenticated retained
historical root under a new candidate generation and restore exact caret and directed selection.
Retention uses a configurable durable byte budget and pins eligible roots until later garbage
collection; no copied inverse text or whole marker registry is authoritative.

Editor realization uses configured retained-memory and per-frame work budgets, priority credits,
bounded filler, and explicit capacity saturation. The shell and renderer retain responsibility for
an unrepresentable drawable surface or framebuffer.

## Affected Authority

The correction is owned by the composer feature and GUI docs, the bounded-resource and Syndic
conversation-history systems, the `beryl-app` and `syndic-storage` package boundaries, and the
external `gpui-text-input` design and widget specification.
