# Scope

Canonical propagation of an owned GPUI revision through Beryl's owned widget dependencies.

# Invalidated Approach

Update only Beryl's direct `gpui` revision after publishing the accepted owned-GPUI commit.

# Evidence

Canonical locked metadata accepted the direct revision, but the focused `beryl-app` check resolved
both the new direct GPUI revision and the prior revision still named by `gpui-scrollbar`,
`gpui-text-input`, and `gpui-settings-window`. Rust then rejected GPUI values and contexts crossing
those package boundaries as different types. The repository-local Cargo path patches conceal this
split by replacing the published dependencies with sibling working copies, so verification under
that local configuration is not canonical pin evidence.

# Why It Failed

Cargo revisions are distinct package sources even when they come from the same repository and
export textually identical Rust definitions. Every owned package that exposes GPUI types must
resolve the same published revision before Beryl can have one coherent GPUI type universe.

# Course Correction

Publish dependency-consistent commits in topological order: `gpui-scrollbar`, then
`gpui-text-input`, then `gpui-settings-window`, and finally update Beryl's GPUI and widget revisions
together. Regenerate and verify each canonical lockfile outside repository-local path-patch scope.
Do not use a Beryl root patch, compatibility wrapper, or local override as the durable correction.

# Affected Authority And Work

- `../plan.md`, Phases 240 through 243.
- `../rework/beryl-home/REWORK.md`, Checkpoint 4's owned-GPUI publication item.
