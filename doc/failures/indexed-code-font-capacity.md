# Failure

Adding a fixed native-face ceiling exposed an existing infallible GPUI failure path. Native glyph
layout could refuse a new fallback face, but the legacy platform wrapper logged the error and
returned empty geometry. Canonical admission accepted its finite zero metrics and advanced the
source without glyphs. Font generation stayed unchanged, so before/after epoch checks could not
detect this failure. Infallible primary-font resolution could also panic when every fallback
attempt failed at capacity. The Code panel's line-metric acquisition initially still used that
resolver before entering its checked layout path.

# Correction

Provide checked requested-font resolution and checked uncached line shaping. Preserve native
capacity errors through glyph callbacks and canonical text/atom admission, rejecting before
continuation, source or geometry publication changes. Keep legacy signatures for their existing
consumers; strict indexed Code uses the checked boundary for row metrics, tab measurement,
oversize presentation and canonical segments. It exposes typed local refusal and retains valid
prior metrics/paint where allowed, rather than treating empty geometry as success.

Pair measured row metrics with the environment key captured around their resolution. Retain that
pair through each panel reconciliation and recompute it before deferred epoch adoption. A current
generation sampled later cannot qualify metrics from an earlier environment. A transient epoch
failure may resume under a stable qualified key; a persistent capacity refusal must not retry
through an unbounded loop. Old FontIds remain immutable and paintable after adoption or refusal.

# Verification Boundary

Focused owned-GPUI tests exercise typed face-cap refusal, unchanged continuations/publication,
native raster preservation, requested-font cache invalidation and qualified streaming epoch
rejection. Mounted tests and native source-anchor qualification additionally cover row metrics,
source remapping and refusal recovery. A generation check establishes input continuity; it never
substitutes for checked operation success.
