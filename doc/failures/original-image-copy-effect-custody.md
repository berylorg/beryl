# Original Image Copy Effect Custody

The first original-command API elected Copy, transferred a raw `ClipboardItem` to the foreground,
and kept acknowledgement and its byte charge in a separate command handle. Independent source
review invalidated that split: prepare, elect, take the item, then drop the command before the
checked write. Generic command cancellation could report a fabricated clipboard failure, zero the
encoded charge and release attachment drain while the transferred bytes/effect were outstanding.
A test that kept the command alive until acknowledgement did not exercise this ownership cut.

The correction transfers a non-cloneable effect owner containing both the prepared item and its
exact operation/charge. It lends the item only to one synchronous checked-write call, retains all
custody through that call, drops the output, then settles the actual acknowledgement. The raw item
cannot escape separately from that obligation. Dropping an uninvoked effect proves no write was
invoked and can cancel after releasing its owned bytes; dropping an unrelated command handle cannot
establish a result. A native error or unwinding call reports failure/uncertainty without success,
rollback inference or automatic effect replay.

The [original-command contract](../systems/image-assets/original-commands.md) owns this boundary.
Qualification must transfer the effect, drop the initiating handle and retire its window/attachment
while the effect is still held, then prove that acknowledgement or explicit uninvoked-effect cleanup
releases the original slot and encoded charge. Independent implementation acceptance and the
passing regression evidence are recorded in the
[command qualification](../audits/original-image-command-qualification.md).
