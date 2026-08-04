# Invalidated Approach

Phase 26 briefly treated `cargo check --tests` or an unfiltered `cargo nextest run -p beryl-app`
as a possible completion gate after removing the backend's materialized dynamic-tool request API.
That gate compiled every Cargo-auto-discovered app test, including historical tests which
path-include deliberately unmounted diagnostic, GUI-control, settings, theme, and transcript
source.

# Why It Failed

Commit `b9e5aba` removed those feature modules from `beryl-app`'s live crate graph while retaining
their test bodies as future regression intent. The old targets still depend on
`DynamicToolCallRequest`, `parse_dynamic_tool_call_request`, cloned argument
`serde_json::Value`, and post-allocation deserialization. Theme and settings cases also retain
whole `String` and `Vec` argument shapes. Adapting or aliasing that request API would reintroduce a
forbidden compatibility path, while deleting only the first four failing targets would leave the
broader intentionally unmounted GUI/theme test island and discard future behavior selectively.

The adjacent `syndic_transcript_manual_scroll` target also path-includes the old diagnostic tool
source, so it belongs to the same eventual disposition rather than an isolated compile fix.

# Architectural Correction

Phase 26 follows the established Phase 25 gate: run the `beryl-app` library suite plus explicitly
named mounted and affected integration targets. Do not shim, rewrite, delete, or `cfg`-hide the
unmounted test island in this bounded CAS-ingress slice. Checkpoint 4 will port still-relevant
diagnostic, settings, theme, and transcript assertions onto their final mounted implementations;
Checkpoint 7 owns removal of every obsolete source and test membership edge that was not replaced.

# Reusable Lesson

During a clean architectural rework, Cargo test discovery can reach source that the live crate
graph intentionally no longer mounts. An all-target build is not automatically an authoritative
gate. Determine test membership from the active checkpoint and mounted package graph; never make a
stale suite green by restoring a removed API or deleting an incomplete subset of future behavior.
