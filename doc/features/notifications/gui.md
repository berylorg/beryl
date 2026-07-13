# Notifications GUI

This is a normative supplemental GUI composition file for `design.md`. It owns notification feature slot mounts, layout relationships, and widget composition. Product behavior, notice queueing, sound eligibility, lifecycle notification policy, and failure handling remain in `design.md`.

## Main Conversation Notices

Mount-into: main-window.overlays

The feature mounts one project-local [`main-window notice`](../../gui/widgets/main-window-notice/spec.md) near the top-right of the conversation window below the toolbar and any visible thread-lineage strip. The widget owns one visible notice's anatomy, bounded selectable detail, close-control placement, overlay anchoring, replacement continuity, warning/error/info treatment, and content-free diagnostics.

The feature supplies at most one active bounded notice record from its FIFO queue, including stable notice identity, content revision, title, detail, variant, and exact dismissal effect. Queue caps, deduplication, coalescing, ordering, and the decision to advance after dismissal remain in `design.md`; queued records are not mounted behind the visible widget.

Replacing or dismissing the visible notice does not shift the toolbar, thread-lineage strip, transcript region, activity panel, composer panel, or status line. When dismissal advances the queue, the feature supplies the next notice as a replacement in the same overlay anchor.
