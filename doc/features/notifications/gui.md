# Notifications GUI

This is a normative supplemental GUI composition file for `design.md`. It owns notification feature slot mounts, layout relationships, and widget composition. Product behavior, notice queueing, sound eligibility, lifecycle notification policy, and failure handling remain in `design.md`.

## Main Conversation Notices

Mount-into: main-window.overlays

Notifications is the only feature that mounts the project-local [`main-window notice`](../../gui/widgets/main-window-notice/spec.md). It mounts one instance near the top-right of the conversation window below the toolbar and any visible thread-lineage strip. Other features contribute owner-configured records to the Notifications arbiter and mount no competing notice widget. The widget owns one visible notice's anatomy, bounded selectable detail, close-control placement, overlay anchoring, replacement continuity, warning/error/info treatment, and content-free diagnostics.

The feature supplies at most one active bounded notice record from its priority queue, including
stable notice identity, content revision, bounded title and detail projections, variant, and exact
dismissal effect. Queue caps, deduplication, priority ordering with FIFO order inside one priority,
and the decision to advance after dismissal remain in `design.md`; queued records are not mounted
behind the visible widget.

Replacing or dismissing the visible notice does not shift the toolbar, thread-lineage strip, transcript region, activity panel, composer panel, or status line. When dismissal advances the queue, the feature supplies the next notice as a replacement in the same overlay anchor.
