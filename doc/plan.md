# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md`. Protect concrete supported-envelope consequences without
duplicating dependency guarantees, runtime validation, or review machinery.

The replacement shell is the final target-state composition boundary, not a compatibility shell or
a reduced copy of the archived workspace-era view. It ultimately mounts every declared main-window
slot and feature contribution, including theme roles, toolbar and lineage, transcript and its owned
scrolling, optional activity and discussion surfaces, composer, status line, overlays, notices, and
Settings entry. Reuse accepted live target-state services, hosts, projections, and widgets; keep
unimplemented mounts visibly absent or unavailable until their owning bounded phase completes.

Use the accepted, canonically pinned single GPUI dependency graph and atomic GPUI window-set
appearance publication when shell implementation resumes. Keep startup restoration,
onboarding, placement, close, Exit, later catalog/navigation/activity/status/notice/settings/
transcript mounts, repair, recovery, branch, asset, integration, and closure boundaries in the
active rework tracker until their own bounded slices are ready.

Marker-operation admission and visible refusal, diagnostic activation, and compact repair-media
implementation and acceptance remain in their owning
rework checkpoints; updating target authority does not mark those behaviors implemented.

# Phase 299: Establish The Bounded Main-Window Notice Arbiter (finished)

Established and independently accepted the bounded per-window notice arbiter with exact identity,
priority/FIFO, protected-condition capacity, stale-action rejection, and bounded display records.
All 16 focused integration tests, the library check, and formatting checks passed. The
[notification record](failures/notifications.md) identifies the retained temporary test config.

# Phase 303: Establish Canonical Notice Theme Roles (pending)

Implement the complete role family declared by the
[notice widget contract](gui/widgets/main-window-notice/spec.md) in the canonical theme schema,
under [theme runtime ownership](systems/theme-runtime/design.md) and
[theme state](../crates/beryl-state/doc/design-theme-settings.md). Verify supported properties,
built-in fallbacks, severity and state variants, and complete resolution with focused schema tests
and semantic review before widget integration.

# Phase 300: Implement The Main-Window Notice Widget (pending)

Implement and verify the existing [notice widget contract](gui/widgets/main-window-notice/spec.md)
against bounded owner-supplied records, independently of feature queue policy and shell mounting.

After the canonical theme roles above, implement the widget-owned bounded selectable detail,
selection/copy, scrolling, commands, focus continuity, and inert input rejection from the existing
contract. Verify same-identity content revisions, exact replacement and dismissal, bounded layout,
diagnostics privacy, and themed rendering through focused GPUI tests and semantic review.
The [readiness correction](failures/main-window-notice-readiness.md) records why an unsuitable
optional text-control reuse candidate is not a widget blocker or a dependency-repair prerequisite.

# Phase 301: Mount The Per-Window Notice Projection (pending)

Connect the accepted arbiter and widget through the
[notification GUI contract](features/notifications/gui.md), with exact window ownership and a
single stable overlay projection. Feature-owned event contributions remain separate where missing.

# Phase 302: Preserve The Resident Editor Through Close Flush (pending)

Establish and verify a close flush that freezes mutations while preserving the resident editor and
read-only interaction until later close obligations settle, under
[composer behavior](features/composer/design.md) and
[ordinary close](features/main-windows/design.md). The existing WindowClose flush disposes the
editor before a subsequent session failure can be known; the
[readiness finding](failures/main-window-close-readiness.md) records why it cannot be used unchanged.

# Phase 298: Mount Ordinary Main-Window Close (pending)

Mount ordinary close under [main-window behavior](features/main-windows/design.md), preserving
the visible window and claim until exact active work, dirty draft, and durable session removal
settle. Await the notice and resident-preserving flush prerequisites above. Verify duplicate close
admission, exact stop and continuation cancellation, failure return
to the coherent open state, independent-window preservation, and final-window empty-restore
termination. Inspect accepted stop, composer, session, and notice dependencies before activating;
split any independently missing component into its own prerequisite phase. Exact-stop and typed
session-removal primitives exist; noninterruptible active-work waiting and close-time continuation
cancellation still require a focused readiness check before activation.

Startup, Exit, restoration, onboarding, and the other deferred mounts remain in the rework tracker.
