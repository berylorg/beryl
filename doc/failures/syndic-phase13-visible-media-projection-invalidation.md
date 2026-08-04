# Syndic Phase 13 Visible Media Projection Invalidation

## Scope

Phase 13 typed provider-item publication for generated-media lifecycle frames.

## Invalidated Approach

The initial live-event mutation used one `is_visible` predicate both to mark transcript output dirty
and to decide whether a revised canonical item owned Markdown projection state that had to be
invalidated.

## Evidence And Failure

Generated media is visible in the transcript, but its canonical presentation owns resource
metadata rather than a `ProjectionTextSource`. When a started image-generation item completed,
the shared visibility predicate sent it through item-projection invalidation. That mutation
correctly rejected the nonprojectable item with `ProjectionBuildConflict`, preventing terminal
capture from preserving a path-pending generated image.

The Phase 13 generated-media integration fixture reproduced the failure through ordinary CAS
item lifecycle notifications. Activity-only items did not fail because they are neither visible
nor text-projectable.

## Why It Failed

Transcript visibility and Markdown projection ownership are different capabilities. User input and
narrative items have both. Generated media has transcript visibility and resource ownership but no
text projection. Operational items may own text projections without using the same transcript-row
classification as generated media.

## Course Correction

Live-event publication continues to use presentation visibility when deciding whether transcript
state became dirty. It now invalidates item-projection authority only when the current canonical
item has an actual `projection_source`.

No empty projection, generated-media text adapter, or special-case projection record was added.

## Authority And Verification

Phase 13 of `doc/plan.md`, the Syndic package design, and the CAS-live Syndic transcript system
design own this boundary. The generated-media integration test proves that completion preserves the
provider frame and pending resource while leaving history intentionally unfinished until asset
ownership exists.
