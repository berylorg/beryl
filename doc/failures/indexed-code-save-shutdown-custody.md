# Code Save Needs Shutdown Custody

The initial mounted Save detached its background writer and relied on view-drop cancellation to
wake staging cleanup. This qualified ordinary retirement while the app executor remained alive,
but supplied no completion witness to final close or Exit. Independent review traced Windows
background dispatch to a detached thread-pool action; GPUI shutdown awaits only registered quit
observers under its own timeout, and Beryl final teardown can call `App::quit` without that Save
writer settling. A slow write or elected publication could therefore be cut off by normal exit.

The owning main-windows feature already requires admitted work and in-flight request handling to
settle before process exit. The bounded correction uses the invoking window's existing running-
resource `WorkerLifetime` witness. Save admits one opaque lease through a weak capability before
native destination choice, transfers it to the writer, and retains it through terminal cleanup or
publication. The existing running-resource drain predicate then awaits that lease before native
window removal and process quit. Suspension fences new admission and cancels pre-election actions;
an elected effect retains its lifetime until complete. A pending native chooser completes normally
without reviving cancelled action state.

The lease retains no Home service, provider or GUI entity. There is no additional registry, worker
pool, effect journal or native-picker cancellation framework. Save admission must leave the two
slots required by the existing `retained()+2<=9` read predicate: after acquiring the lease, total
retained work is at most seven. Leaving only one slot would block Save at the eight-witness edge.
Ordinary resume paths must reopen the same admission fence; changing only `reads_suspended` leaves
Save disabled after otherwise healthy activation. Terminal release precedes completion delivery/capacity observation. Qualification must exercise the
actual root suspension/drain path with held picker, source write/cleanup and elected publication,
and prove post-fence refusal and terminal release. A background executor's continued existence
alone is not graceful-shutdown custody.
