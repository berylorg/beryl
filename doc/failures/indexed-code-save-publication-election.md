# Code Save Publication Election

The initial Save proposal required retirement to preserve the destination until the physical
rename completed, while also requiring nonblocking GUI retirement and off-GUI filesystem work.
Inspection of the worker publication boundary showed those requirements cannot be enforced by
holding a cancellation mutex across rename: foreground retirement would wait for an OS call.
An atomic cancellation flag alone cannot prevent a rename already admitted on another thread.

The owning transcript feature and app adapter authority instead define one publication election
after exact current-source/action qualification. Cancellation or retirement that wins before this
election prevents replacement. A publication elected first finishes its authorized atomic effect,
even if its view retires before the OS operation returns. All staging writes, close, publication and
owned-file cleanup remain off GPUI. No foreground destructor removes a staging file or waits on a
filesystem mutex. Failed publication retains destination preservation; cleanup failure remains an
explicit failure rather than evidence of complete disposal. A real Windows deny-delete handle
in focused run `1349e954-9be0-46fe-8235-eaf9b4eaa8bd` proved that an owned staging file can remain
despite closing the writer. The owning feature and adapter permit that distinct terminal failure
with one residue; they do not promise unconditional deletion or add automatic cleanup retries.

Acceptance must exercise both election outcomes through the mounted command, plus actual complete
file bytes, publication failure and staging disposal. The election resolves invocation timing; it
adds no general effect journal, automatic retry or claim of crash recovery.
