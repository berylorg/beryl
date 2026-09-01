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

# Why It Failed

Cargo revisions are distinct package sources even when they come from the same repository and
export textually identical Rust definitions. Every owned package that exposes GPUI types must
resolve the same published revision before Beryl can have one coherent GPUI type universe.

A coherent type universe and successful compilation do not prove behavioral compatibility for a
dependent fork. An accepted GPUI revision can require package-source reconciliation even when its
intended feature is unrelated to that package's public contract, and local path overrides can hide
both source-identity and published-revision behavior from canonical verification.

# Course Correction

Publish dependency-consistent commits in topological order: `gpui-scrollbar`, then
`gpui-text-input`, then `gpui-settings-window`, and finally update Beryl's GPUI and widget revisions
together. Regenerate and verify each canonical lockfile outside repository-local path-patch scope.
Do not use a Beryl root patch, compatibility wrapper, or local override as the durable correction.

At each dependent fork, treat its focused package-contract integration targets as a publication
gate. When a revision-only update fails that gate, stop the dependency-only phase and establish a
separate package-source compatibility investigation and acceptance boundary before publication;
do not weaken or replace the failing tests merely to continue propagation.

# Affected Authority And Work

- `../plan.md`, Phases 241 through 244 and the blocked Phase 242 source-compatibility decision.
- `../rework/beryl-home/REWORK.md`, Checkpoint 4's owned-GPUI publication item.
