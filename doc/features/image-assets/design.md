# Goals

Let users attach images to drafts, recognize them through stable conversation labels, preview them,
submit them, copy or save them, and revisit them after restart or branching.

Make generated media visibly preserve one identity while it moves from pending admission to either
an admitted asset-backed result or a terminal unavailable result.

## Non-goals

- Providing a standalone image browser or cleanup screen.
- Rendering draft images as inline thumbnails instead of compact markers.
- Defining asset storage, reference records, filesystem paths, paging, runtime projection, decode,
  cache, GPU, or repair mechanics.
- Defining where composer or transcript controls are mounted.

# Decisions

## Authority And GUI Contracts

- This feature owns user-visible image attachment, marker, preview, generated-media, Copy, and
  `Save…` behavior.
- The [image-asset system](../../systems/image-assets/design.md) owns durable admission, generated
  output handoff, storage, reference, runtime, rendition, and recovery mechanics.
- The [composer design](../composer/design.md) and its [GUI composition](../composer/gui.md) own
  draft-marker workflows and placement. The [transcript design](../transcript/design.md) and its
  [GUI composition](../transcript/gui.md) own transcript placement and turn-level presentation.
- The [image marker](../../gui/widgets/image-marker/spec.md) and
  [image preview](../../gui/widgets/image-preview/spec.md) specifications are normative for their
  reusable rendering, interaction, contextual-command, and focus mechanics.

## Paste And Draft Outcome

- Pasting supported image clipboard content enters a visible paste-pending state before any marker
  appears. Draft mutation and submission remain temporarily unavailable while that exact paste is
  pending, and `Escape` may cancel it before admission.
- Successful admission inserts one compact marker with its final conversation label at the captured
  insertion or replacement range and records one undoable edit. Failure or cancellation leaves the
  prior draft, markers, labels, selection, and caret unchanged.
- A clipboard representation that cannot be admitted safely is reported as unavailable for image
  paste before it changes the draft.
- Manually authored text that resembles an image marker remains ordinary text. Copying or cutting a
  selection that contains markers writes explanatory text for those markers; a live private marker
  representation may preserve an attachment when pasted back into an eligible Beryl draft.

## Marker Identity And Labels

- Every admitted marker has one stable identity and one final visible label in its conversation.
  The label remains unchanged through autosave, submission, accepted or queued input, retry,
  restart, transcript projection, branching, and replacement editing.
- Repeated references to the same admitted image may retain the same label within one conversation.
  Pasting into another conversation allocates that conversation's own label.
- Label allocation never renames later markers or guesses a label. If the next safe label cannot be
  established, admission fails visibly and leaves the draft and existing labels unchanged.
- Removing a draft marker removes only that occurrence. Removing the last draft-only occurrence
  does not make accepted or historical markers disappear.

## Marker And Preview Presentation

- Admitted composer and transcript attachments remain visible as compact labeled markers rather
  than becoming ordinary text when their image cannot currently be shown.
- Activating an eligible readonly transcript marker opens image inspection without editing or
  replacing the image and without opening an external viewer. Activating an editable composer
  marker instead opens its configured `View` and `Remove` menu; choosing `View` opens inspection.
- Inspection shows a bounded fitted rendition when ready, a stable pending presentation while that
  rendition is being prepared, or a local unavailable presentation when the image cannot be loaded,
  decoded, or displayed within the active presentation limits.
- A local unavailable outcome preserves the exact marker identity and label. It never removes the
  marker, changes the draft or transcript, or silently substitutes another image.
- Closing image inspection returns focus to its exact origin when that origin remains eligible and
  otherwise to a stable fallback chosen before inspection opened. Image inspection is unavailable
  when no fallback can remain eligible for its lifetime.

## Generated-Media Identity Lifecycle

- One generated-media item keeps the same visible identity across all of its states. Provider
  completion does not create a second visible item when asset admission finishes.
- `Pending` begins before durable asset admission. The transcript keeps a stable pending media
  presentation while the exact provider handoff is still eligible for admission or recovery.
- `Admitted` begins only when that same item is backed by a durable admitted asset. Its visible
  presentation may independently be `Rendition pending`, `Ready`, or `Locally unavailable` without
  changing the item's admitted identity.
- `Unavailable` is terminal when the generated output has no recoverable `savedPath` handoff or its
  asset admission cannot be recovered. The same item remains visible with an unavailable outcome;
  it never disappears, changes identity, or falls back to another byte source.
- A repaired terminal turn exposes generated images only with the repaired turn's one complete
  publication. If any generated image required by that repair reaches terminal unavailable, the
  turn is visibly `Incomplete` and no sibling image is presented as a partially repaired result.

## Shared Copy And Save Command Matrix

- This matrix applies to ordinary pasted or attached images and generated-media items wherever
  their contextual commands are available. An ordinary attachment enters the matrix only after
  admission. A generated-media item enters in `Pending` and keeps the same command target identity
  if it becomes `Admitted` or terminal `Unavailable`.
- Copy and `Save…` are expected commands for every image in the matrix and remain visible throughout
  that image's applicable states.
- The command origin is the exact eligible marker, media item, preview command anchor, or other
  stable control from which the contextual command surface opened. Every success, cancellation,
  decline, or failure returns focus to the command origin when it still exists and otherwise to a
  stable fallback chosen before the surface opened; the surface is unavailable when no fallback can
  remain eligible for its lifetime.
- Activating any disabled Copy or `Save…` command performs no command and leaves focus on that
  disabled command, regardless of the disabled reason or media state.
- In generated-media `Pending`, Copy and `Save…` remain visible but disabled with the reason that
  the original image has not completed admission.
- `Ready` is an `Admitted` presentation substate, not a second image identity. Ordinary admitted
  attachments and generated media that are both `Admitted` and `Ready` use the same command
  outcomes below.
- In `Admitted` with the original locally readable, Copy is enabled when the exact original image
  has a supported platform clipboard representation and that complete representation fits the
  platform clipboard limit. Rendition-pending presentation does not disable Copy merely because a
  fitted preview is not ready.
- When the exact original lacks a supported clipboard representation or exceeds the clipboard
  limit, Copy remains visible but disabled and explains that specific reason.
- Successful Copy places the exact original image on the platform clipboard in that supported
  representation. Clipboard unavailability, ownership rejection, or platform write failure reports
  Copy failure, leaves the media state unchanged, and never reports a partial representation,
  preview rendition, stale rendition, or substitute image as copied.
- In `Admitted` with the original locally readable, `Save…` is enabled independently of preview
  readiness. It opens the platform file picker for a user-selected destination. Cancelling the
  picker creates no file, changes no existing file, reports no success, and returns focus according
  to the command-origin rule.
- A display-only `Locally unavailable` presentation does not disable Copy or `Save…` while the exact
  original remains locally readable.
- If the selected destination already exists, `Save…` requires explicit overwrite confirmation
  before changing it. Declining overwrite leaves the existing destination unchanged, reports no
  success, and returns focus according to the command-origin rule.
- Successful `Save…` leaves the exact original bytes at the user-selected destination and reports
  success only after that complete result exists. Destination-open, permission, capacity, or write
  failure reports Save failure and leaves an existing destination byte-for-byte unchanged or a new
  destination absent. No partial file, preview rendition, stale rendition, transformed substitute,
  or other destination content is reported as saved successfully.
- In `Admitted` with the original temporarily unreadable, Copy and `Save…` remain visible but
  disabled with the local unavailable reason. The admitted identity remains unchanged.
- In terminal `Unavailable`, Copy and `Save…` remain visible but disabled with the terminal reason.
  They never target a fallback byte source.
- After either admitted command becomes enabled, a source-read or supporting-runtime failure reports
  that command as failed, leaves the admitted image identity unchanged, and never reports partial or
  substitute output as successful.
- A clipboard, destination-write, or file-picker failure likewise never changes an admitted image's
  identity or reports command success. For generated media still in `Pending`, a provider or runtime
  failure leaves the item pending only while exact admission recovery remains available; otherwise
  the same item becomes terminal `Unavailable` under the lifecycle above.

## Submission And Historical Outcomes

- Submission sends the image represented by each admitted marker with its stable visible label and
  preserves the marker's order relative to surrounding text and other markers.
- Image preparation failure proven before acceptance rejects submission with the draft, marker
  identities, labels, caret, and selection intact.
- Image-asset access or runtime preparation failure after acceptance preserves the accepted marker
  and history identity, reports the post-acceptance failure, and never recreates the input as an
  editable draft.
- Accepted, queued, retryable, delivery-unknown, submitted, and historical markers keep their exact
  visible identity and label across restart and later delivery-state changes.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `external-side-effects/v1`
- `irreversible-operation/v1`
