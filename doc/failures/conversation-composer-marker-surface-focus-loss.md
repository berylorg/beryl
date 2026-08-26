# Conversation Composer Marker Surfaces Cannot Treat Focus Loss As Realization Loss

## Scope

Phase 181 editable image-marker activation, where a focused range-backed text input opens an
anchored command menu and may transfer that interaction into an image-preview overlay.

## Invalidated Approach

Mount the menu in Beryl and let its focus move away from the external text input while preserving
the existing rule that every text-input focus loss clears the active inline object. A Beryl-side
exception that ignored the resulting `FocusLost` realization-loss event was also considered.

## Decisive Evidence

The menu must own focus to provide truthful keyboard, dismissal, and modal-preview behavior. The
external widget previously cleared its exact active object on that same focus transition and
emitted realization loss, so the canonical owner immediately closed the menu it had just opened.
Ignoring that event in Beryl would keep a surface alive after the widget had released its identity
and geometry authority, making later removal, replacement, or unrealization indistinguishable from
an ordinary menu focus transfer.

## Course Correction

- Give the external range widget one exclusive, non-cloneable attachment token bound to the exact
  realized anchor.
- Preserve the active object across focus loss only while that token is retained.
- Move the token from marker menu to image preview without a release gap.
- Invalidate attachment custody on removal, replacement, layout supersession, unrealization, and
  disposal, and keep their ordinary exact realization-loss events authoritative.
- On dismissal, validate the token and either refocus the exact origin or clear it before applying
  the owner-supplied stable fallback.
- Retain only one fixed-size attachment and no marker registry or overlay-owned realization cache.
