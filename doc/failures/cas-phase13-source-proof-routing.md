# Scope

Phase 13 app-to-connection replayable text page routing.

# Invalidated Approach

The first capacity-one broker request carried only `TextSourceProof`, source-relative start, and
maximum bytes. The broker intended to use that caller-visible proof as the sole selector for a
retained text run.

# Evidence And Failure

The initial backend contract required only that a proof remain stable for one source. It did not
state that equal proofs imply equal bytes, declared length, provenance, or backing range. Two
same-length runs could therefore carry the same proof while naming different authored segments or
generated fragments. A page from the wrong run could still satisfy proof, start, size, and progress
checks. The broker shape also left conversion between source-relative offsets and Syndic's
content-absolute segment offsets implicit.

A content proof or digest may validate immutable provenance, but it is not a safe request-local
routing handle and must not become digest-only byte-equality authority.

# Required Course Correction

- Assign every prepared text run one request-local unique broker source id.
- Route each page request by that private id into one exact retained run descriptor.
- Carry and validate `TextSourceProof` separately; its contract binds exact immutable logical bytes,
  declared length, and provenance for the complete request.
- Make every run descriptor explicitly map source-relative offsets onto generated fragments or an
  opaque Syndic segment proof and content-absolute range.
- Reject duplicate or mismatched ids, proofs, lengths, ranges, and page coordinates before returning
  a page.

The source id is transient orchestration state, not a durable identity or CAS wire field. The app
broker remains capacity one and retains no whole authored text.

# Affected Authority And Proof

The correction updates the CAS-live system, backend and app package docs, root plan, and active
rework tracker. Focused proof has two separate boundaries:

- Valid replayable sources that carry equal proofs must retain equal bytes, declared length, and
  provenance while their distinct request-local ids remain observable by the broker.
- A deliberate equal-token collision may be constructed only from raw internal broker envelopes,
  below the `ReplayableTextSource` contract boundary, to prove that routing selects the exact
  request-local id rather than the proof. Those adversarial envelopes are not valid replayable
  sources and must never be presented as such.
