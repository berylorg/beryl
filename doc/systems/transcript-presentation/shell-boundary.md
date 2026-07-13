# Transcript Shell Boundary

## Status

This is the normative system boundary for the shell-facing Syndic-backed transcript presentation stack.

It defines the target surface that Beryl shell code uses during transcript activation, rendering, scrolling, diagnostics, selection, quote, menus, and media actions. It does not define Syndic storage, durable history, provider internals, or migration phases.

## Target Shape

Beryl shell owns one transcript host per conversation surface.

The transcript host is the shell's single boundary for transcript history presentation, viewport, residency, release, navigation, selection, branch/edit provenance, media, and diagnostics. Shell code interacts with it through host-owned commands, snapshots, facts, and diagnostics instead of reaching into those subsystems separately.

The GPUI transcript panel is a render consumer of host snapshots. It may report viewport, geometry, selection, widget, and resource demand facts back to the host. It never requests Syndic or `syndic-storage` data directly.

The exact Rust names are implementation details, but the live boundary should be conceptually:

- a shell-owned transcript host state object on the selected conversation surface;
- a renderer-facing resident presentation snapshot;
- a demand-fact sink used by the panel, scroll controller, and nested widgets;
- a provider-facing residency owner that is the only Beryl transcript layer allowed to request Syndic data;
- narrow shell commands for activation, scrolling, explicit navigation, copy, quote, context menus, media actions, popup closure, diagnostics, and retained-state reporting.

## Owned State

The transcript host owns selected transcript-view identity, activation revision, resident Syndic data, presentation data, applicable synthetic-context contributions, realized frame window, scroll anchor, manual scroll state, live-tail state, pending demand facts, resource pins, active selection pins, open menu pins, local transient affordances, fallback records, presentation revisions, cancellation generations, and transcript diagnostics.

The host does not own Syndic canonical history, durable storage, `syndic-storage`, backend execution state, Beryl-home persistence, composer draft state, status-line chrome state, activity-panel records, or backend thread inventory.

## Inputs

The host accepts selected transcript-view activation seeds, previously coherent host state for non-blank activation handoff, typed synthetic-context contribution descriptors, provider responses, invalidation notices, viewport size, theme-dependent measurement changes, manual scroll deltas, explicit navigation requests, live-turn events, renderer demand facts, nested-widget resource demand, selection changes, quote requests, copy requests, context-menu requests, media action requests, popup-close commands, and diagnostic read commands.

Activation seeds name the selected transcript view and requested initial placement. A branch-discussion seed may also name the immutable context-owner identity, expected revision, and insertion parent needed for transcript residency to read and derive one stable synthetic-context group. Seeds do not carry selected context text, legacy history-window objects, or legacy presentation-state objects.

Provider responses carry Syndic transcript-view cursor pages, immutable branch-context envelopes, projection records, resource metadata, resource ranges, revisions, and rejection or stale-result state. They do not carry legacy transcript models.

## Outputs

The host publishes immutable resident presentation snapshots for the GPUI panel. A snapshot contains only currently resident presentation records, realized synthetic-context chunks, stable group and record identities, realized frame data, provenance, local fallback records, local affordances, and enough revision data for the renderer to reject stale measurements or demand facts.

The host also publishes shell-facing status facts, turn-view facts, retained-state diagnostics, transcript-frame diagnostics, visible-media diagnostics, media lifecycle events, copy payloads, quote payloads, context-menu targets, media action targets, and scroll-command outcomes.

The host emits provider demand through the Beryl-facing Syndic provider contract only. It does not expose `syndic-storage` handles to shell code or renderer code.

## Demand Facts

Renderer-driven residency is indirect.

The panel, scroll controller, and nested widgets report demand facts to the host: visible presentation range, overscan range, missing leading or trailing range, current semantic anchor, measured geometry, manual scroll direction, explicit navigation target, live-tail intent, resource range demand, active selection pin, open menu pin, media preview pin, copy or quote pin, obsolete resident range, and stale measurement or revision observations.

The host evaluates those facts under residency policy. It decides what to load, retain, pin, evict, release, reject, cancel, or retry. Rejections become stable fallback or clamp state rather than unbounded memory growth.

## Diagnostics

Diagnostics describe resident Syndic data and presentation state, not legacy transcript internals.

Required diagnostics include resident record counts, realized frame range, visible range, synthetic-context group and realized-chunk counts, estimated resident bytes, resource bytes, decoded or uploaded media budget, active pins, pending provider requests, stale provider results, rejected demands, fallback records, release decisions, current anchor, scroll mode, activation revision, and presentation revision.

Diagnostics may expose Syndic provenance and transcript-view positions. They must not require loading nonresident history or total transcript height.

## Invariants

- New code must not preserve legacy transcript type names as shell compatibility shims.
- Shell code must not inspect transcript residency, presentation, viewport, or resource internals outside the host boundary.
- Renderer code must build GPUI elements only from resident presentation snapshots.
- Renderer code, nested widgets, and shell commands must not call Syndic storage or `syndic-storage` directly.
- The host may request Syndic data only through the Beryl-facing provider contract.
- Missing data, pending data, stale data, rejected data, and loading state are not transcript content.
- Synthetic context remains presentation-only, occupies its immutable branch-boundary position, does not increment turn counts, and cannot become a quote, branch, edit, or ordinary turn-menu target.
- Activation publishes content and initial viewport state atomically when a coherent seed is available.
- A previous coherent transcript may remain visible until the new coherent seed is ready.
- Manual scrolling remains exact pixel displacement and never snaps to turns, rows, chunks, or transcript boundaries.
- Resident Syndic data may be released only when current anchor, visible content, active selection contract, and UI pins stay valid.
- Selection, copy, quote, menus, and media actions operate only on rendered resident records with stable provenance and geometry.
- Budget fallbacks are local UI records tied to Syndic provenance, not assistant-authored transcript content.
