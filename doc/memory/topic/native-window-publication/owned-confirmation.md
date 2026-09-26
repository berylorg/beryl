# Reason For Investigation

Determine which native dependency is needed for exact running-shutdown confirmation, duplicate
reveal and disposal tracking before ordinary close/Exit composition.

# Outcome

GPUI's Windows generic prompt captures a raw parent handle, uses severity-derived titles, installs
no TaskDialog callback and returns only a button index. Its public wrapper may fall back to a GPUI
overlay. It therefore does not supply the required owned native confirmation lifecycle.

TaskDialogIndirect supports explicit title/default button and native creation/destruction
notifications. Its callback receives the exact dialog handle; TDN_DESTROYED says that handle is no
longer valid. Cancellation can use the Cancel button message, and creation plus modal-call return
can distinguish native-open failure from completed disposal. The operation must not report a
positive selection before disposal, or confuse loss of its result channel with cancellation proof.

GPUI's foreground dispatcher posts work to its platform window, whose procedure drains foreground
runnables. A native modal loop can dispatch those messages, so the modal call must run outside app
and window entity borrows. The current foreground dispatcher uses flume `drain()`, which removes
the whole queue into an outer iterator; a native modal call strands later tasks in that batch.
Nested message dispatch therefore needs pending tasks to remain in the receiver until execution.
Native operation state must also release borrows before message-sending
calls that may reenter. Real native testing is necessary to qualify this integration.

The existing hidden-window lease is restricted to unpublished windows and cannot be reused for a
published confirmation owner. Native removal already records deferred destruction and has an exact
WM_NCDESTROY receipt; confirmation must coordinate with that path rather than poll raw handles or
assume holding an Rc alone prevents native destruction. Target contracts belong to the fork root
design and Beryl app lifecycle; this note is source evidence only.

# Sources

- Beryl-owned GPUI, `https://github.com/berylorg/zed-fork`, commit
  `19ef0796613226c123a3c9047865060e7639f713`, inspected 2026-09-27:
  `crates/gpui/src/window.rs` prompt and native operation APIs;
  `src/platform/windows/window.rs` prompt;
  `src/platform/windows/native_operation.rs` lifetime and destruction;
  `src/platform/windows/dispatcher.rs` dispatch_on_main_thread and
  `src/platform/windows/platform.rs` run_foreground_task.
- Microsoft, [Task Dialog](https://learn.microsoft.com/en-us/windows/win32/controls/task-dialogs),
  updated 2020-10-23, accessed 2026-09-27; creation/destruction notifications and button messages.
- Microsoft, [TASKDIALOGCONFIG](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/ns-commctrl-taskdialogconfig),
  accessed 2026-09-27; explicit title, default Cancel, cancellation flags and callback state.
- Microsoft, [TaskDialogIndirect](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-taskdialogindirect),
  accessed 2026-09-27; native modal call and result boundary.
