# Scope

Checkpoint 3 Phase 13 capture of the pinned Codex App Server 0.144.1 public turn-item union.

# Invalidated approach

The first closed-disposition implementation treated several public operational and activity item
variants as fieldless activity markers. It retained item identity, kind, lifecycle, and sometimes
one concatenated text value, but discarded the rest of the normalized public payload.

# Evidence

The normalized backend union already preserves the public fields of command execution, file
changes, MCP and dynamic-tool calls, collaboration and subagent activity, web search, image view,
sleep, image generation, review-mode transitions, hook prompts, agent citations, and reasoning
summaries. The app descriptor boundary reduces many of those variants to an empty activity payload
or one untyped text stream, and the storage schema cannot encode the discarded structure. Terminal
audit then accepts that coarse payload as complete.

This permits a provider-complete turn to become history-complete after losing command metadata,
file paths and change boundaries, tool arguments and results, collaboration identities, media
provenance, review text, or other public fields required by the pinned contract.

# Why It Failed

An explicit disposition is not sufficient when that disposition is lossy. A fieldless activity
marker cannot be authoritative for a public item whose normalized variant carries data. Concatenated
text also cannot preserve list boundaries, field identity, indices, option presence, or structured
values. Treating those records as complete makes later transcript, recovery, and audit behavior
depend on information Syndic no longer owns.

# Course Correction

Every admitted pinned public item must have a closed typed structural representation. Arbitrarily
large strings and structured leaves remain in bounded canonical content chunks; immutable typed
manifests name their exact fields, order, optionality, and content ranges without copying the bytes
into source-event or canonical-item metadata. MCP and dynamic-tool structured values use a closed
typed value algebra rather than raw JSON or an opaque blob.

“Admitted” is narrower than the upstream wire object: fields deliberately discarded by the backend
ingress contract are not silently lost Syndic data. In particular, standalone image-generation
base64 `result` never enters the normalized union; `doc/failures/cas-phase13-image-result-persistence.md`
records that separate correction.

Start and completion publish immutable typed snapshot generations, with the completion snapshot
authoritative for final public fields. Completion-only variants retain their exact typed payload
without an invented start event. A terminal audit may publish history-complete only when the final
snapshot is sealed, structurally complete, kind-consistent, and every referenced content frontier is
durable. Malformed, missing, unsupported, or unresolved payloads preserve the exact provider outcome
but carry a typed history-incomplete reason.

# Affected Authority

`doc/plan.md` Phase 13, `doc/rework/beryl-home/REWORK.md`,
`doc/systems/cas-live-syndic-transcript/design.md`,
`doc/systems/syndic-conversation-history/design.md`, `crates/beryl-app/doc/design.md`, and
`crates/syndic-storage/doc/design.md` carry the corrected target contract.
