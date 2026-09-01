# Scope

Canonical propagation of an owned GPUI revision through Beryl's owned widget dependencies.

# Invalidated Approach

Update only Beryl's direct `gpui` revision after publishing the accepted owned-GPUI commit, then
assume each dependent fork can propagate that revision through manifest and lockfile changes alone.

# Evidence

Canonical locked metadata accepted the direct revision, but the focused `beryl-app` check resolved
both the new direct GPUI revision and the prior revision still named by `gpui-scrollbar`,
`gpui-text-input`, and `gpui-settings-window`. Rust then rejected GPUI values and contexts crossing
those package boundaries as different types. The repository-local Cargo path patches conceal this
split by replacing the published dependencies with sibling working copies, so verification under
that local configuration is not canonical pin evidence.

After `gpui-scrollbar` successfully propagated the accepted revision, the canonical
`gpui-text-input` manifest and lockfile update resolved one GPUI universe and passed locked metadata,
package compilation, and all 18 prepublication integration cases. Its focused `range_widget`
integration target nevertheless failed three existing cases: adjacent zero-width objects were not
realized, concurrent clipboard/geometry residency returned `Busy`, and committed inline-object
replacement exhausted exact geometry with `ExactGeometryWrongPage` validation rejections.

Exact isolated comparison later used identical committed package source, exact prior and accepted
manifests and locks, separate target directories, and one canonical command. Both graphs produced
the same empty-object, clipboard-busy, and wrong-page symptoms in all three repetitions. The earlier
residency-capacity and request-shape report was not reproducible under that baseline and came from
non-exact command, source, configuration, or target evidence. A source-only diagnosis around the
coalesced geometry-response transition was also invalidated when reverting that transition, and
then all behavioral changes from its introducing commit, left the accepted-pin failures unchanged.

An exact identical-source lifecycle comparison found only the accepted GPUI window's correct initial
active fact before the first request cycle. After normalizing that fact, both graphs produced the
same content-free lifecycle, wait, custody, continuation, and request-count traces. Explicitly
drawing and draining the accepted window's initial active frame left all three cases failing across
nine bounded runs, so activation and test-driver drain sequencing are not the correction owner.

The next response-custody trace found a real cross-purpose resident-object classification mismatch,
but the sole bounded candidate that removed purpose equality from resident lookup left every named
case unchanged across three repetitions. It therefore did not establish the correction boundary.
The replacement case separately proved that deterministic wrong-input and stale-alignment geometry
responses remain dispatched and are requeued through repeated wrong-page rejection; package-source
custody violates the atomic terminal-failure contract, but the smallest delivery, shared-closure, or
custody-service correction locus remains unproven. Overlapping-object realization remains
unattributed after resident selection. Clipboard begins while a same-binding target still awaits an
object page, exposing an unresolved authority gap between the test's committed-surface availability
expectation and the source's noninteractive-target `Busy` behavior.

# Why It Failed

Cargo revisions are distinct package sources even when they come from the same repository and
export textually identical Rust definitions. Every owned package that exposes GPUI types must
resolve the same published revision before Beryl can have one coherent GPUI type universe.

A coherent type universe and successful compilation do not prove behavioral compatibility for a
dependent fork. An accepted GPUI revision can require package-source reconciliation even when its
intended feature is unrelated to that package's public contract, and local path overrides can hide
both source-identity and published-revision behavior from canonical verification.

Matching failing test names across revisions do not prove one causal mechanism. Historical source
substitution is not an exact canonical comparison when the historical manifest, lock, or dependency
lifecycle differs, and a suspicious authority-contradicting branch is not the demonstrated cause
until reversing it changes the observed failure.

A real state-classification mismatch is likewise not the demonstrated cause of a named failure when
the exact candidate that reverses it leaves that failure unchanged. Deterministic response failure
cannot use mere continued dispatch presence as proof of retryable custody when authority permits
retention only for explicit surface-publication capacity.

# Course Correction

Publish dependency-consistent commits in topological order: `gpui-scrollbar`, then
`gpui-text-input`, then `gpui-settings-window`, and finally update Beryl's GPUI and widget revisions
together. Regenerate and verify each canonical lockfile outside repository-local path-patch scope.
Do not use a Beryl root patch, compatibility wrapper, or local override as the durable correction.

At each dependent fork, treat its focused package-contract integration targets as a publication
gate. When a revision-only update fails that gate, stop the dependency-only phase and establish a
separate compatibility investigation and acceptance boundary before publication. Compare exact
canonical graphs at their first content-free lifecycle divergence and exercise a bounded candidate
correction before selecting package source, test driver, or owned dependency as the implementation
owner; do not weaken or replace failing tests merely to continue propagation.

When an excluded divergence leaves the correction owner unresolved, split the next independently
verifiable diagnostic boundary instead of extending the rejected candidate. Establish a repeatable
baseline and follow the first response-validation or custody mismatch before selecting implementation
scope.

After the exact baseline and sole failed candidate, do not broaden the existing phase with another
correction attempt. First correct the missing clipboard command-eligibility authority, then plan
separate bounded evidence for deterministic response closure, clipboard gating and residency, and
resident-object scanner continuation and publication. Keep publication blocked until each actual
correction owner and independently implementable boundary is established.

# Affected Authority And Work

- `../plan.md`, Phases 241 through 244, including Phase 242's activation exclusion and Phase 243's
  unresolved response-validation and custody diagnosis.
- `../rework/beryl-home/REWORK.md`, Checkpoint 4's owned-GPUI publication item.
