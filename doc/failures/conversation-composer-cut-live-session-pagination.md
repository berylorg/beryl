# Conversation Composer Cut Cannot Page Through A Mutating Live Session

## Scope

Phase 181 composite Cut after the platform clipboard write, where marker removals are converted
from bounded cursor pages into bounded proposal pages.

## Invalidated Approach

Read one marker page through the selected live composer session, stage its removal proposal page,
then read the next marker page through the same selected session. The implementation retained only
one proposal page and fixed boundary facts, but assumed the live session remained a valid read
authority after proposal staging.

## Decisive Evidence

A nine-marker focused GPUI Cut forced a second marker page. The first proposal page was accepted,
then the continuation read failed exactly with
`DraftPieceRangeSourceErrorV1::StaleSession`. Proposal staging advances the mutable candidate
session, so its predecessor binding can no longer qualify a later source read. The failure occurs
before mutation commit and is independent of clipboard capacity or retained-memory limits.

The same test also exposed a distinct adapter mismatch: Syndic marker edges identify adjacent
unreturned markers, while `gpui-text-input` page edges identify the exclusive request or
continuation cursor. Passing those edge values through directly produced
`ObjectContractError::MalformedContinuation` on backward same-anchor paging.

## Course Correction

- Translate Syndic adjacent edge proofs into the widget's request-cursor and continuation-cursor
  page-edge contract.
- Qualify continuation reads against the immutable mutation predecessor root retained by the active
  mutation, not the mutable live candidate session after proposal staging.
- Continue retaining only one bounded page plus fixed previous/current/lookahead marker facts; do
  not restore whole-selection vectors or retain a second proposal page as a workaround.
- Keep final commit and successor proof selection-qualified against the exact active mutation and
  current slot identity.
