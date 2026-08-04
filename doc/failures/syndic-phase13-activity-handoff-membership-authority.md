# Activity Handoff Row And Terminal Range Need One Authority

## Context

One parent activity-query row may summarize a direct child thread's exact final answer after that
child is terminal. The membership starts at the final-answer source event and ends at the terminal
source frontier.

## Invalidated Approach

The first terminal-only implementation checked that the nominated item was a valid final answer and
that the child was terminal, but did not prove the final-answer item event was immediately followed
by the terminal event. A later activity-visible item could therefore fall inside the membership
interval even though the bounded handoff mutation wrote only one row. The successful mutation would
then fail its own reopen completeness rules.

Reopen also validated the membership's handoff item/range and the entry's compact handoff fact
independently. Coherent corruption could change the entry to another semantically valid narrative
subrange without matching the membership authority.

## Accepted Correction

Publication and reopen both require the nominated final-answer item's current source event plus one
to equal the proven terminal source-event frontier. Existing turn validation proves that frontier is
the exact `TurnEnded` event. A child with later provider work is not eligible for the single-row
handoff; the mutation does not scan or backfill an arbitrary interval.

The retained handoff entry must exactly equal the source membership's child thread, final-answer
item, and narrative range. Semantic validity alone is insufficient. Focused mutation and coherent-
corruption tests cover both boundaries.

Publication also requires the exact child source-membership key to be absent. It inserts one new
inactive membership beginning at the final-answer event and ending at the proven terminal
frontier. A preexisting membership is a typed conflict; refreshing it could strand earlier rows
outside the rewritten range and has no production authority.
