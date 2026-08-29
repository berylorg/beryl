# Composer Private Clipboard Provenance Cannot Accumulate At Final Write

## Invalidated Approach

Expose every selected inline object's fallback-output span as one collection attached to the final
contiguous clipboard write request. Beryl could then build its private marker token from that
collection without duplicating the widget's merge algorithm.

## Decisive Evidence

`gpui-text-input` bounds clipboard text bytes and each object page, but it intentionally permits a
logical selection to contain arbitrarily many source-zero-width objects. An object's fallback may be
empty, so the contiguous text-byte limit does not bound the number of provenance facts. Retaining all
facts until the final write would therefore make widget memory proportional to the complete marker
selection and violate the accepted range-backed clipboard boundary.

## Accepted Correction

The app-neutral coordinator streams exact ordered provenance in bounded pages with
selection-qualified cursors, ordinals, canonical cumulative identity, checked output offsets, and
positive item and retained-byte ceilings. The final capped text write carries only compact stream
closure. Consumers may incrementally stage or discard those pages, while the widget retains one page
and fixed cumulative state and releases custody on every terminal path.

The controlling implementation boundary is Phase 192 of `doc/plan.md`; target package and widget
contracts live in `gpui-text-input/doc/design.md` and
`gpui-text-input/doc/gui/widgets/text-input/spec.md`.

## Remaining Risks

The protocol must reject skipped, repeated, reordered, stale, mismatched, over-limit, or
post-cancellation provenance pages; exact empty fallbacks and multiple same-anchor objects must still
advance by item cursor rather than output bytes.

## Post-Mutation Surface Admission Is Invalid

The first ownership-accounting implementation recomputed the widget surface charge after
`RangeClipboardCoordinator::admit_text_page` or `admit_object_page` had already appended output,
cloned object fallback payloads, and possibly allocated provenance storage. Rejection was logically
correct but too late to enforce a hard peak-memory bound: a one-under configuration could allocate
beyond its cap transiently before returning `SurfaceCapacity`.

Precomputing that growth in the widget is also invalid because exact growth depends on the
coordinator's interleaved source-covering atom, zero-width object, output, and provenance merge. That
would create the second merge algorithm expressly forbidden by Phase 192. Moving response ownership
before admission would instead lose the retained retry boundary.

The Operator approved the clean material phase expansion: a coordinator-owned allocation-free
prepare/commit lifecycle. The single merge authority prepares an opaque step and exact peak charge
without consuming response custody; the widget admits that charge; commit then performs the
prepared allocation and state transition without replaying merge semantics.
