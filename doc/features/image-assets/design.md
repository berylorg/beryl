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

- Pasting image clipboard content preflights format and any exact platform/provider constraint
  available without ownership, then streams bytes of arbitrary logical length into durable asset
  admission before inserting the final labeled marker. Dimensions and frame count guide later
  decode availability but never authorize a full decode or become Beryl memory-safety limits. The
  composer uses its paste-pending behavior for a non-immediate admission. Failure or cancellation
  publishes neither a partial asset reference nor a marker.
- A platform clipboard representation that cannot expose a safe size or bounded read boundary is
  unavailable for image paste rather than being decoded or copied whole and checked afterward.
- A marker never enters a durable draft with a provisional label or only transient clipboard bytes.
- The durable marker retains its final label across autosave, accepted-input queueing or steering, submission, replacement, restart, and transcript projection.
- Removing the final draft-only marker removes that draft reference and may release its label according to the composer contract, but it does not delete the home asset bytes.
- Accepted, queued, retryable, delivery-unknown, or submitted marker references keep their exact marker identity, asset id, and label across restart. Moving one accepted input between delivery dispositions or terminalizing an ambiguous request does not replace its references.
- Clipboard reconstruction may reuse a live asset id; ordinary text that resembles an image marker remains text.

## Labels

- Labels are allocated in the target Syndic thread's history scope and remain stable for every reference to the same asset under that label.
- Allocation uses the thread's compact durable label frontier plus its bounded current-draft marker
  index. It never enumerates historical turns, accepted inputs, or every prior label.
- Beryl never reuses a label when validated Syndic evidence says it may already identify another asset.
- Cross-thread paste allocates a label in the destination thread even when the home-wide asset bytes deduplicate to the same asset id.
- CAS receives generated visible label text adjacent to the corresponding image input because CAS image records do not own Beryl labels.

## Preview And Transcript

- Composer and transcript markers expose the existing View behavior over the original durable asset
  identity. Presentation uses an admitted thumbnail, visible tile set, or local unavailable state;
  viewing never requires decoding or uploading the full original merely to fit the preview.
- Missing, corrupt, unsupported, oversized, or temporarily unavailable bytes leave the marker and label visible with a local unavailable state.
- Preview failure never removes the marker, rewrites the draft, queries CAS history, or substitutes another same-digest-looking file without proof.
- A generated image becomes transcript media only after Beryl reads the CAS-provided `savedPath`
  through the exact runtime and admits those bytes as a home asset. Missing or unreadable output is
  shown as unavailable; Beryl never falls back to the protocol's discarded base64 result.
- Copy uses a platform clipboard representation only when its exact encoded output fits the admitted
  contiguous limit. Otherwise Copy is unavailable and `Save…` streams the original durable asset to
  the selected file without routing bytes through renderer pixels, a thumbnail, or a whole command
  payload.

## Runtime Submission

- Host and WSL submissions both use the same durable asset identity.
- Beryl verifies the canonical Host sidecar and derives either that exact Host path or its direct `/mnt/<drive>` WSL projection before accepting submission.
- Input submission creates no runtime staging file or cache. The derived runtime path is transient request data, not another asset or user-visible history.
- Failure to verify the Host sidecar or derive the exact runtime path rejects submission with the draft and marker intact.

## Reference Removal And Cleanup

- No image-asset bytes are deleted before a future explicit Collect Garbage design proves
  reachability and shared-reference safety; this includes removal of a named Syndic thread, draft
  reference, transcript reference, or Beryl catalog copy.
- Branching and replacement editing preserve every historical asset reference on immutable turns.
- Assets that become unreachable remain in the home store until the future explicit Collect Garbage design proves reachability and shared-reference safety.
- No implicit startup, archive, delete, retry, preview, or cache cleanup operation performs durable asset garbage collection.
