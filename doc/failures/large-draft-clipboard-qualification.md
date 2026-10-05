# Large-Draft Clipboard Qualification

## Canonical Cargo Working Directory

An initial check selected the canonical manifest from the main checkout's working directory.
Cargo still loaded that working directory's ignored local configuration and selected local widget
patches. The run was interrupted and is excluded from acceptance. Run Cargo from the canonical
checkout itself, with only the explicit bounded build configuration and shared target directory.
The replacement canonical locked app/executable all-target check passed.

Interrupting the PowerShell qualification wrapper did not immediately stop its Cargo/nextest/test
descendants. The root kept the serial Cargo permit, verified their exact parent chain and waited
for its bounded drain before starting another run. Wrapper completion alone is not descendant
completion. The interrupted invalid-footer run is excluded from acceptance.

## Failed-Generation Source Inspection

An initial review suggestion treated the private source `descriptor` accessor as a cached-state
getter and requested descriptor equality after physical storage failure. The accessor validates
healthy live generation authority and expires an invalid source; calling it cannot establish usable
failed-generation clipboard authority. Review withdrew that assertion after checking the existing
home failure and replacement contracts.

Healthy size/capacity refusals must preserve the complete descriptor through Notice dismissal.
Storage refusal instead proves exact cached cut root/history/presentation and bounded custody,
without old-generation reads, retry or Undo. Ordinary retirement/replacement expires the source.
Retain the source owner through normal window teardown to verify origin-source token expiry.

## Window Disposal And Process Owner Retirement

An initial review suggestion required the process clipboard owner to become retired after the
window fixture's normal teardown. That fixture constructs window services directly and does not
run complete process-graph shutdown. Composer disposal expires its exact origin source while
the shared process owner can continue serving other windows. Review corrected the assertion:
retain the owner across window teardown, verify the token no longer resolves and the weak
composer/input/service handles are released. Whole-graph owner retirement remains separately
qualified through the accepted process shutdown and recovery boundaries.

## Nonresident Marker Precondition

Complete durable marker facts and low resident text bytes do not prove that a particular marker
was nonresident. The corrected fixture explicitly checks that the origin marker is absent from the
realized object set while the selected tail marker is present, then preserves exact durable marker
identity across the cut and later refused paste.

## Selected Marker Fallback Identity

The initial large-cut fixtures assumed the selected tail marker's fallback was `[Image A]`.
The real mounted write produced `[Image B]`: the selected marker's authenticated label controls
its representation. Derive the exact expected fallback from that selected marker's label, and
compare both the checked writer's complete value and its captured immutable item. Do not use
the first marker's label or a presentation assumption to qualify a different selected marker.
