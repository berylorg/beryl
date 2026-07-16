# Goals

Let users paste, preview, submit, copy, and revisit images across durable Syndic drafts and transcripts without tying asset storage to an execution root or project directory.

Keep image labels stable per conversation while ensuring branching, restart, Host execution, WSL execution, and any later thread-reference removal never depend on temporary clipboard bytes or CAS historical reads.

## Non-goals

- Providing a standalone image-asset browser or cleanup screen.
- Rendering draft images as inline thumbnails instead of compact markers.
- Deleting asset bytes when a marker, draft, thread, or Beryl metadata record is removed.
- Implementing the future Collect Garbage workflow.
- Treating a filesystem path or CAS image record as durable image identity.

# Decisions

## Ownership

- Image bytes live in one Beryl-home-wide content-addressed asset store.
- Drafts, accepted input, submitted turns, queued input, retries, transcript markers, and clipboard payloads hold typed references to stable asset ids.
- A thread does not physically own or duplicate an asset merely because it references it.
- The owning Syndic history boundary remains authoritative for which labels have appeared in one thread; Beryl asset metadata remains authoritative for resolving an asset id to bytes.

## Paste And Draft Lifetime

- Pasting image clipboard content validates count and byte limits, durably admits the asset and draft reference, and only then inserts its final labeled marker.
- A marker never enters a durable draft with a provisional label or only transient clipboard bytes.
- The durable marker retains its final label across autosave, accepted-input queueing or steering, submission, replacement, restart, and transcript projection.
- Removing the final draft-only marker removes that draft reference and may release its label according to the composer contract, but it does not delete the home asset bytes.
- Accepted, queued, retryable, delivery-unknown, or submitted marker references keep their exact marker identity, asset id, and label across restart. Moving one accepted input between delivery dispositions or terminalizing an ambiguous request does not replace its references.
- Clipboard reconstruction may reuse a live asset id; ordinary text that resembles an image marker remains text.

## Labels

- Labels are allocated in the target Syndic thread's history scope and remain stable for every reference to the same asset under that label.
- Beryl never reuses a label when validated Syndic evidence says it may already identify another asset.
- Cross-thread paste allocates a label in the destination thread even when the home-wide asset bytes deduplicate to the same asset id.
- CAS receives generated visible label text adjacent to the corresponding image input because CAS image records do not own Beryl labels.

## Preview And Transcript

- Composer and transcript markers expose the existing View behavior over the original durable bytes.
- Missing, corrupt, unsupported, oversized, or temporarily unavailable bytes leave the marker and label visible with a local unavailable state.
- Preview failure never removes the marker, rewrites the draft, queries CAS history, or substitutes another same-digest-looking file without proof.
- A generated image becomes transcript media only after Beryl reads the CAS-provided `savedPath`
  through the exact runtime and admits those bytes as a home asset. Missing or unreadable output is
  shown as unavailable; Beryl never falls back to the protocol's discarded base64 result.

## Runtime Submission

- Host and WSL submissions both use the same durable asset identity.
- Beryl validates a runtime-readable path to the exact bytes before accepting submission.
- Runtime staging is an implementation cache, not another durable asset and not user-visible history.
- Failure to prepare or verify the runtime path rejects submission with the draft and marker intact.

## Reference Removal And Cleanup

- No image-asset bytes are deleted before a future explicit Collect Garbage design proves reachability and shared-reference safety; this includes removal of a named thread, draft reference, transcript reference, or Beryl thread metadata.
- Branching and replacement editing preserve every historical asset reference on immutable turns.
- Assets that become unreachable remain in the home store until the future explicit Collect Garbage design proves reachability and shared-reference safety.
- No implicit startup, archive, delete, retry, preview, or cache cleanup operation performs durable asset garbage collection.
