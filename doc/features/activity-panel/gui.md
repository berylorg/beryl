# Activity Panel GUI

This is a normative supplemental GUI composition file for `design.md`. It owns activity-panel slot mounts, layout relationships, and widget composition. Product behavior, runtime activity periods, row content authority, retention, and metadata availability remain in `design.md`.

## Activity Panel

Mount-into: main-window.activity-panel

The feature mounts one project-local [`activity panel`](../../gui/widgets/activity-panel/spec.md) as the optional bounded panel below the transcript region and above any discussion-status strip and composer. The widget owns resize geometry, fixed-height rows, bounded realization, scrolling, truncation, stable row reconciliation, tooltip anchoring, and content-free diagnostics.

The feature supplies the selected thread's current runtime activity period identity, one
revision-bound query identity, total logical row count, bounded resident row pages, stable activity
identities, running-first recent ordering, status-marker state, bounded `Agent` and `Activity` row
projections, and the initial top-attached viewport policy defined in `design.md`. It answers the
widget's bounded page requests without supplying a complete activity collection. The feature also
supplies ready, inert-reconciling, and hidden composition states; their product meaning remains in
`design.md`.

The feature maps each design-owned initial-query or page-query failure to the widget's bounded
panel-level feedback region. It supplies the exact failed query identity, bounded explanation,
`Retry` command identity and availability, pending state, and terminal result. The widget owns only
the command's reusable presentation, disabled/loading mechanics, and focus settlement; it never
chooses retry eligibility or scope.

The feature supplies the design-owned committed window-local panel height and the current minimum
and maximum bounds from the conversation layout. Showing the widget takes height only from the
transcript region; hiding it unmounts the slot contribution without moving the discussion-status
strip, pinned composer, or global status line.
