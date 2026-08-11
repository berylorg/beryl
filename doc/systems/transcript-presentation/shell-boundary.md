# Transcript Shell Boundary

## Status

This file is the normative supplemental shell-facing boundary governed by
`doc/systems/transcript-presentation/design.md`.

It defines the target surface that Beryl shell code uses during transcript activation, rendering,
scrolling, diagnostics, focus, selection, quote, menus, and media actions. It does not define Syndic
storage, durable history, or provider internals.

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

The transcript host owns selected transcript-view identity, activation revision, resident Syndic
data, presentation data, applicable synthetic-context contributions, repair publication generation
and provenance, realized frame window, scroll anchor, manual scroll state, live-tail state, pending
demand facts, the single transcript-view focus-pin slot, resource pins, active selection pins, open
menu pins, local transient affordances, fallback records, presentation revisions, cancellation
generations, and transcript diagnostics.

The host does not own Syndic canonical history, durable storage, `syndic-storage`, backend execution state, Beryl-home persistence, composer draft state, status-line chrome state, activity-panel records, or backend thread inventory.

## Inputs

The host accepts selected transcript-view activation seeds, previously coherent host state for non-blank activation handoff, typed synthetic-context contribution descriptors, provider responses, invalidation notices, viewport size, theme-dependent measurement changes, manual scroll deltas, explicit navigation requests, live-turn events, renderer demand facts, nested-widget resource demand, selection changes, quote requests, copy requests, context-menu requests, media action requests, popup-close commands, and diagnostic read commands.

Transcript-visible live-turn text events carry exact routed thread, turn, item, normalized kind, ordered text, and logical-frontier facts. The host may publish their bounded transient suffix before durable Syndic projection catches up, but it may not reinterpret them as durable history or replay them through a presentation timer.

Activation seeds name the selected transcript view and requested initial placement. A
branch-discussion seed may also name the immutable context-owner identity, expected revision, and
insertion parent needed for transcript residency to read and derive one stable synthetic-context
group. Seeds do not carry selected context text or alternate transcript presentation models.

Provider responses carry Syndic transcript-view cursor pages, immutable branch-context envelopes,
projection records, resource metadata, resource ranges, revisions, and rejection or stale-result
state. Every request and response is keyed by exact transcript-host identity, activation generation,
selected authority generation including any repair-publication generation, requested logical or
byte range, and one unique request id. A repair-publication response additionally names one selected Syndic transcript-
view generation, the exact affected turn and projection revisions, snapshot-backed source
provenance when repaired, and one pending, repaired, or incomplete disposition.

The host does not accept a CAS repair response, repair adapter handle, live suffix, outage buffer,
GUI-local text, or partial repair record as repair-publication input. Live-turn events remain a
separate bounded transient input and cannot populate repaired canonical records.

## Outputs

The host publishes immutable resident presentation snapshots for the GPUI panel. A snapshot contains only currently resident presentation records, any bounded transient live suffix attached to its exact authored record, realized synthetic-context chunks, stable group and record identities, realized frame data, provenance, local fallback records, local affordances, and enough revision data for the renderer to reject stale measurements or demand facts.

For repair publication, one snapshot generation atomically selects the affected turn's exact
Syndic generation, compact head, source authority, pending, repaired, or incomplete provenance, and
optional noninteractive record-status provenance. It does not contain a whole resident turn. The
host emits the new generation after the bounded pages and resource descriptors needed for its
coherent realized window are ready; other pages and indexed source or resource ranges load on
demand under the same generation. Publication switches those turn selectors together and emits no
partial or mixed-authority output.

The repair-generation switch simultaneously removes the superseded generation's transient live
suffix and provisional turn-local evidence. Repair provenance, record content, presentation
revision, and invalidation facts publish together; no separate GUI status output can advance them.
After anchor rebasing, the switch releases the superseded projection pages, presentation records,
measurements and layout, widget state, resource slices, pins, and every associated capacity charge.

The host also publishes shell-facing status facts, turn-view facts, retained-state diagnostics,
transcript-frame diagnostics, visible-media diagnostics, media lifecycle events, copy payloads,
quote payloads, context-menu targets, media action targets, focus-transfer outcomes, and
scroll-command outcomes.

The host emits provider demand through the Beryl-facing Syndic provider contract only. It does not expose `syndic-storage` handles to shell code or renderer code.

## Demand Facts

Renderer-driven residency is indirect.

The panel, scroll controller, and nested widgets report demand facts to the host: visible
presentation range, overscan range, missing leading or trailing range, current semantic anchor,
measured geometry, manual scroll direction, explicit navigation target, live-tail intent, resource
range demand, the bounded focus pin, active selection pin, open menu pin, media preview pin, copy or
quote pin, obsolete resident range, and stale measurement or revision observations.

The host evaluates those facts under residency policy. It decides what to load, retain, pin, evict, release, reject, cancel, or retry. Rejections become stable fallback or clamp state rather than unbounded memory growth.

The host accepts a provider result only while its complete host, activation, authority-generation,
range, and request-id key remains one outstanding current demand. A stale or cancelled result is
discarded and releases every returned page, range, reservation, pin, and response-buffer charge.
Removing the last demand for a keyed range cancels its work and releases its materialized pages,
presentation records, measurements and layout, widget state, resource slices, pins, and capacity
charges when no other current fact owns them. Eviction and authority supersession perform the same
release after any atomic replacement cut.

Retirement of the exact transcript-provider service generation cancels and joins its requests,
closes their response sinks, and releases in-flight capacity. A surviving host may retain only its
one bounded last coherent resident snapshot as inert presentation under the same transferred
charges; that handoff has no request authority. Host disposal or owning-window close releases the
snapshot and all other resident and in-flight state. Every later completion is rejected.

### Focus-Pin Fact

The host exposes exactly one focus-pin slot for the transcript view in its owning window. The slot
is absent when transcript content does not own focus. Otherwise its one fact names the selected
transcript-view identity, stable presentation-record identity, presentation revision, owning
host/repair-publication generation identity, and, only for focus inside a nested widget, the
nested-widget identity and revision. Direct record focus has no nested-widget fields. A focus
move within the transcript replaces this fact atomically; it never appends another fact.

The host accepts and honors the fact only while every identity and revision matches the current
coherent resident snapshot. An invalid or stale fact is rejected, releases its minimum retained
state, and requests focus transfer through the feature-owned fallback chain in
`doc/features/transcript/gui.md`. The host must not infer continuity from equal text, equal visible
labels, record position, nested-widget type, or a semantic-anchor rebase.

The fact may retain only its exact presentation record, the minimum focused resource range, and
the minimum state for its exact nested widget. It counts as at most one active focus pin for the
view and window under `doc/systems/bounded-resource-dataflow/design.md`.

Focus transfer outside the transcript, nested-widget blur or transfer, focused-content removal or
replacement without exact identity continuity, selected-thread switch, window close, and host
disposal release the fact. A destination elsewhere in the same transcript is installed as the one
replacement fact only after releasing the previous nested target.

## Diagnostics

Diagnostics describe only resident Syndic data and current presentation state.

Required diagnostics include resident record counts, realized frame range, visible range,
synthetic-context group and realized-chunk counts, estimated resident bytes, resource bytes, decoded
or uploaded media budget, active pins, focus-pin presence, focus-pin state (`absent`,
`direct-record`, or `nested-widget`), focus-pin accept, reject, release, and transfer counts, pending
provider requests, stale provider results, rejected demands, fallback records, release decisions, current anchor, scroll
mode, activation revision, and presentation revision.

Focus-pin diagnostics contain no focus target identities, transcript content, labels, resource
bytes, or nested-widget content.

Diagnostics may expose Syndic provenance and transcript-view positions. They must not require loading nonresident history or total transcript height.

## Invariants

- Shell compatibility adapters for alternate transcript type names are prohibited.
- Shell code must not inspect transcript residency, presentation, viewport, or resource internals outside the host boundary.
- Renderer code must build GPUI elements only from resident presentation snapshots.
- Renderer code, nested widgets, and shell commands must not call Syndic storage or `syndic-storage` directly.
- The host may request Syndic data only through the Beryl-facing provider contract.
- Missing data, pending data, stale data, rejected data, and loading state are not transcript content.
- Synthetic context remains presentation-only, occupies its immutable branch-boundary position, does not increment turn counts, and cannot become a quote, branch, edit, or ordinary turn-menu target.
- Activation publishes content and initial viewport state atomically when a coherent seed is available.
- A previous coherent transcript may remain visible until the new coherent seed is ready.
- Every arrived bounded fragment of a normalized transcript-visible text delta becomes eligible for
  the next GUI frame without synthetic character pacing. Fragments already pending at one frame
  boundary may publish together without losing parent-delta identity or order.
- A transient live suffix remains bounded and non-authoritative. Exact Syndic prefix agreement transfers that range to durable presentation without duplication, blanking, or identity substitution; mismatch fails closed.
- Repair publication accepts only one complete selected Syndic generation with generation-owned
  pending, repaired, or incomplete provenance; CAS repair snapshots, live or outage state, GUI
  text, and partial repair data never become host replacement content.
- The host atomically switches the affected turn's generation, compact head, source authority, and
  provenance while retaining only bounded demanded pages and ranges. It never emits a mixed-
  generation or blank intermediate turn and disposes any superseded transient live suffix in that
  same switch.
- Every asynchronous provider result matches its complete host, activation, authority-generation,
  range, and request-id key or is discarded with all returned capacity released; host, window, and
  provider-service disposal cancel all outstanding demand.
- Last-demand removal, eviction, supersession, host disposal, and window close release all
  materialized state and its charges. Provider-service retirement may transfer only one bounded
  inert last coherent snapshot to a surviving host and transfers no loading authority.
- Repair status and record-status provenance are immutable within one host generation and cannot update
  independently through shell or renderer state.
- The focus-pin slot has capacity one per transcript view and owning window; invalid or stale facts
  release retained state and transfer focus through the feature-owned fallback chain.
- Manual scrolling remains exact pixel displacement and never snaps to turns, rows, chunks, or transcript boundaries.
- Resident Syndic data may be released only when current anchor, visible content, active selection contract, and UI pins stay valid.
- Selection, copy, quote, menus, and media actions operate only on rendered resident records with stable provenance and geometry.
- Budget fallbacks are local UI records tied to Syndic provenance, not assistant-authored transcript content.
