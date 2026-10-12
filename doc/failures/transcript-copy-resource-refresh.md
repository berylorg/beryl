# Transcript Copy Resource Refresh

A shared keyboard cancellation hook originally treated all outer transcript geometry invalidation
as loss of the selected source. Native Code Copy then cancelled itself when delivery of its own
source page staged a fresh outer frame. The writer was never reached; the native fixture timed out
with `Copy unavailable: keyboard target changed` despite current source and ready layout.

Outer geometry invalidation revokes ordinary record selection. It does not by itself retire a Code
source operation. Code selection/root/revision checks still govern source publication, while a
later keyboard request supersedes an unfinished earlier Copy. Focus changes break the press pair
without cancelling a valid accepted source read solely because focus changed.

A retained keyboard panel id also incorrectly referred to a later independent header-button Copy
in the same action slot. Resetting pairing on the header focus change cancelled that new command.
The pairing owner therefore retains no pending Code action pointer; source actions retain custody,
and a new keyboard Copy explicitly supersedes pending copies.

GPUI blur subscriptions describe active rendered focus paths. They do not report every raw focus
change in an inactive native window. Observe the raw focus handle at render and keyboard entry;
rendered departure and return then invalidate the pair without treating a stale paint-path event
as a new focus change. Native test scenarios must also complete their retained fixture teardown.

The focused native Code selection test guards this interaction, including a repeated key and a
second distinct press before the source read finishes. Windows reported the fixture's timeout
panic crossing its native callback as abort code `0xc0000409`; that report was not evidence of
clipboard buffer corruption.

The accepted shared Copy boundary and final native regression evidence are recorded in the
[mounting audit](../audits/transcript-mounting-readiness.md#shared-transcript-keyboard-copy-acceptance).
