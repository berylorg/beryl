# Layout Replay Must Preserve New Structural Range Demand

After vertical scrolling, a mounted Code window may retain valid indices for overlapping rows
while its new logical range still needs descriptors. Retention does not establish that the new
structural request has been admitted.

The native history-revision fixtures moved from logical range 0..16 to 2..18 with horizontal
offset 45.5. Fourteen old indices survived. Their source seeds were correctly released on range
replacement, so replaying them immediately needed Display reads. The coordinator issued Display
before the new Range reply arrived, superseding the structural request for rows 16 and 17.
After the known fourteen rows finished, no remaining query requested the two missing rows.
Diagnostics stayed pending with fourteen painted rows and a Display reply instead of the required
Range. All three native history-revision timing cases exposed this route; long-line origin-only
qualification did not.

The correction distinguishes retained indices from admission of the current logical range.
Layout parks until an exact-binding Range request covers that range, leaving the ordinary
structural demand active. A matching partial Range counts as admitted: its known long line may
then need Display replay, followed by its exact continuation or next logical descriptor request.
Vertical range or source-binding replacement clears both admission and the prior continuation.
No new loader, retry queue or inferred descriptor supplies the missing rows.

The same custody rule applies after a layout epoch disposes all retained descriptors and seeds.
An ordinary completed multi-page Range can leave the viewport's saved input cursor pointing at
its final tail page. Copy Source responses then replace the host's Range pages. Reusing that
terminal cursor after font invalidation re-admits only the tail while its original request bounds
still name the whole range. In the native source-action/font case, range 50..66 appeared admitted,
but only lines 64 and 65 survived while the retained inline anchor belonged to line 52. No remaining
query could reconstruct it, leaving pending geometry indefinitely.

Descriptor/seed disposal therefore marks a bounded structural restart at cursor `None` before
cached continuation replay. Preserve the source anchor, exact owner/root and shared charges; let
ordinary Range admission reconstruct the bounded window. Keep the exact continuation during
same-epoch partial reads whose leading pages or indices remain retained. Completed request bounds
are not proof that all descriptor pages are resident.

The [layout contract](../systems/transcript-presentation/renderer-architecture.md#indexed-no-wrap-code-layout)
and root plan remain authoritative. Keep the native history-revision timing cases in verification
when changing layout/structural demand priority. Qualification of distant horizontal traversal
alone cannot prove vertical-window replacement.
