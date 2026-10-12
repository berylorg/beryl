# Waiting For A Source Response Is Not Deferred Work

The mounted Code action coordinator initially reported work and notified rendering whenever a
Copy or endpoint-remap query remained pending, even when the exact query was already outstanding
and no matching response could advance it. Rendering scheduled another deferred action step,
which notified again. The bounded source buffer and finite query size did not bound this wakeup
loop.

The focused native selection/Copy run exposed foreground executor non-quiescence: both native
cases stayed active without yielding to their internal deadlines. Their exact owned test children
were stopped after 141.607 and 72.008 seconds; five focused state cases passed, while terminal hit
resolution required a separate correction. The interrupted native cases are not acceptance.

Schedule another action step only for a real query transition or a matching admitted response
that can advance the action. Ordinary resource publication supplies the next wakeup while a read
is outstanding. Keep bounded canonical layout quanta separate from waiting command state, and
retain source, owner and cancellation qualification around publication and effects.

The affected coordinator and mounted scheduling gate are in the Code panel subtree. Corrected
native acceptance and disposal belong in the active [plan](../plan.md). This is independent of the
[terminal refusal lesson](indexed-code-source-action-refusal.md): even a healthy asynchronous read
must not create an unconditional render/deferred-work loop.
