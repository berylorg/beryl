# Scope

Incremental autosave, undo, and submission over a revision-bound Syndic current draft.

# Invalidated Approach

Use `ComposerV1` as the current-draft autosave backing: first through the existing complete-payload
API, then through a proposed bounded streaming rebuild of one complete successor from the prior
revision plus exact edits.

# Evidence

`PreparedContent::composer` accepts one complete `ComposerPayload` and builds resident vectors of
all canonical chunks, text spans, and content pieces. `ContentAppend::prepare` accepts that complete
`PreparedContent` and copies the next bounded chunk batch from it. Although the internal ComposerV1
fold accepts bounded text fragments and emits bounded records, its atom-writer and record-sink
traits are reachable publicly only through test support, not through the production crate API.

`DraftPayloadUpdate::prepare_reference` can publish an already sealed successor reference, but no
public production boundary constructs that sealed ComposerV1 successor by streaming the prior
revision plus exact range edits.

The proposed streaming correction would bound input and output residency, but every save would
still read the complete prior composer and write a complete new ComposerV1 successor. Its I/O and
work therefore remain linear in total draft size even when one small range changed.

# Why It Failed

The existing path reconstructs and retains the complete draft and marker collection. Bounded final
chunk appends do not repair that upstream whole-value allocation. The proposed fully streamed
successor removes the allocation but still performs full-draft read/write amplification on every
frequent autosave, making a small edit to a very large draft cost proportional to unchanged
content. Neither approach satisfies the large-draft editing contract.

# Course Correction

Use a durable immutable copy-on-write composite piece tree as the current-draft backing. Autosave
applies ordered non-overlapping text and zero-width-marker edits against one exact base root, writes
only inserted leaves and affected path copies in bounded steps, and atomically selects one complete
revision-scoped successor root. Revision conflict, cancellation, supersession, crash, retry, and
orphan staging remain explicit without exposing a partial draft.

Keep `ComposerV1` as submitted/canonical content, not autosave state. Submission and other named
canonical consumers separately stream one exact immutable piece-tree root into bounded unreachable
ComposerV1 staging, verify and seal it, and consume only that root-bound sealed result.

# Affected Work

The controlling conversation-history and `syndic-storage` designs own the piece-tree lifecycle,
storage-neutral composite transaction contract, concrete records and bounds, and separate
ComposerV1 materialization boundary. The active implementation plan and Beryl editor integration
must derive from those authorities rather than reintroducing a full-successor autosave path.
