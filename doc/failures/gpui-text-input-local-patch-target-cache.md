# Scope

Local sibling-fork verification of `gpui-text-input` before the accepted GPUI composite-layout
revision is published and canonically pinned.

# Invalidated Approach

Run canonical-GPUI and local-patched-GPUI Cargo builds in the same `gpui-text-input` target
directory and assume Cargo will always keep their same-version artifacts isolated.

# Evidence

During Phase 131, a read-only review compiled the canonical pinned GPUI graph into the ordinary
target directory. The next correctly configured local-patch run resolved `gpui` to
`../zed-fork/crates/gpui` but failed with 110 missing old/new streaming-layout API errors. The owned
fork source and composite symbols were intact and unchanged. Cleaning only the `gpui` and
`gpui-text-input` package artifacts through the local configuration restored the build, after which
the focused tests passed.

# Why It Failed

The canonical and path-patched graphs use the same package name and version while exposing
different unpublished APIs. Reusing one target directory across those graphs can leave consumer
artifacts paired with the wrong dependency surface even though metadata resolves the intended path.

# Course Correction

Until Phase 133 publication and canonical pinning remove the temporary graph split, do not run a
canonical-GPUI build in the same target directory used for local sibling acceptance. If a mixed
build already occurred, verify the resolved manifest path and intact fork source, then clean only
the exact `gpui` and `gpui-text-input` package artifacts before rerunning the established local
configuration. Do not modify dependency source or invent a compatibility API.

# Affected Work

Phase 131 local verification recovered with the narrow package cleanup. Phase 133 owns publication,
canonical acceptance, and elimination of this temporary dual-graph hazard.
