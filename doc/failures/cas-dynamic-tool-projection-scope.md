# CAS Dynamic-Tool Projection Scope

Phase 10 initially allowed each CAS projection request to carry complete thread-start options while
still preferring native current, resume, and fork lineage. That implied Beryl could establish a
feature-specific dynamic-tool set on whichever native path the coordinator selected.

Codex App Server 0.144.1 disproved that assumption. `dynamicTools` is an experimental
`thread/start` field only. The exact generated `ThreadResumeParams` and `ThreadForkParams` schemas,
the tagged protocol source, and the official app-server documentation expose no dynamic-tool
replacement or addition at resume, fork, turn start, or thread-settings update. Beryl's initial
coordinator consequently forwarded tools for fresh and recovered starts but silently omitted them
for native resume and fork, while the already-loaded path ignored the request options entirely.

The invalid approach would make tool availability depend on lineage and would force frequently used
discussion branches either to lose their resolution tool or to reconstruct a cache-unfriendly fresh
history projection. Registering a per-discussion tool after a native fork is not a supported CAS
operation, and `thread/inject_items` injects history items rather than tools.

The Operator selected a cache-stable course correction. Every persistent Beryl conversation CAS
thread starts with one canonical, versioned, deterministically ordered Beryl tool registry. Native
fork and resume may be used only with proof of the same registry version. Registration is not
authorization: each feature-owned handler still validates the exact CAS thread, turn, call,
Syndic/Beryl target, feature state, and revision before mutation, and a wrong-scope request fails
without state change. The discussion-resolution tool is therefore registered on ordinary Beryl
conversation threads but authorized only on an exact open discussion projection.

Phase 10 must prove against the exact admitted CAS executable that provider-facing dynamic-tool
definitions survive fork and restart/resume unchanged before relying on this architecture. Failure
of that proof blocks implementation; it does not authorize routine recovery injection, a global
permission bridge, repeated history replay, or another compatibility workaround.

The retained proof passed with the final canonical tagged registry. The complete provider-facing
tool array remained byte-identical through initial start, inclusive fork, and process
restart/resume: 11,021 bytes, SHA-256
`A86607BB83A2378E7F7470985B3EAEC526E38255975AD1ABECF07F5F4FFFBD02`.

Affected authority and implementation are `doc/features/branch-discussions/design.md`,
`doc/systems/branch-discussion-handoff/design.md`,
`doc/systems/cas-live-syndic-transcript/design.md`, `crates/beryl-app/doc/design.md`, and Phase 10
of `doc/plan.md`.

Evidence was collected from installed `codex-cli 0.144.1`, SHA-256
`D3D92E9C10A6F3371A425214C3DF67EB97EC5C2FF1B88876410FE0E61D4791DA`, its stable and
experimental generated app-server schemas, the OpenAI Codex App Server documentation, and the
`rust-v0.144.1` tagged protocol source.
