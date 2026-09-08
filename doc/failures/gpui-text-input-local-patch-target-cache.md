# Scope

Cargo artifact provenance when verifying sibling forks or copied isolated checkouts with a shared
target directory. The original incident involved `gpui-text-input` before the accepted GPUI
composite-layout revision was published and canonically pinned.

# Invalidated Approach

Run canonical-GPUI and local-patched-GPUI Cargo builds in the same `gpui-text-input` target
directory and assume Cargo will always keep their same-version artifacts isolated. The same
assumption is unsafe when copied source files preserve timestamps older than another checkout's
build.

# Evidence

During Phase 131, a read-only review compiled the canonical pinned GPUI graph into the ordinary
target directory. The next correctly configured local-patch run resolved `gpui` to
`../zed-fork/crates/gpui` but failed with 110 missing old/new streaming-layout API errors. The owned
fork source and composite symbols were intact and unchanged. Cleaning only the `gpui` and
`gpui-text-input` package artifacts through the local configuration restored the build, after which
the focused tests passed.

During marker-continuation verification, an untouched `4235eb9` baseline compiled and passed two
default-stack cases. Switching to the changed isolated checkout reused those executables in
0.22 seconds and appeared to pass. Source hashes differed, but executable timestamps, sizes and
measured preparation frames still matched the baseline. Refreshing only the task-owned isolated
source timestamps forced a 19.16-second rebuild; both cases then exposed the actual stack overflow.
The apparent isolated pass was discarded.

# Why It Failed

The canonical and path-patched graphs use the same package name and version while exposing
different unpublished APIs. Reusing one target directory across those graphs can leave consumer
artifacts paired with the wrong dependency surface even though metadata resolves the intended path.
Likewise, source-path intent and a successful command do not prove that copied source with older
timestamps produced the executed binary.

# Course Correction

Until Phase 133 publication and canonical pinning remove the temporary graph split, do not run a
canonical-GPUI build in the same target directory used for local sibling acceptance. If a mixed
build already occurred, verify the resolved manifest path and intact fork source, then clean only
the exact `gpui` and `gpui-text-input` package artifacts before rerunning the established local
configuration. Do not modify dependency source or invent a compatibility API.

For copied-checkout verification, prefer a distinct task-owned target directory. If a shared target
is deliberately reused, force freshness for the exact copied package before switching source
variants and verify that the intended source was rebuilt. Refreshing task-owned source timestamps
must leave content hashes unchanged. Check executable identity or measured code when comparing
variants; discard results from reused wrong-source artifacts. Do not clean an ambiguous shared
target tree to repair a task-local provenance problem.

# Affected Work

Phase 131 local verification recovered with the narrow package cleanup. Phase 133 owns publication,
canonical acceptance, and elimination of this temporary dual-graph hazard.
The marker-continuation stack comparison requires independently rebuilt baseline and changed
executables; an enlarged-stack run of the wrong executable is not diagnostic evidence.
