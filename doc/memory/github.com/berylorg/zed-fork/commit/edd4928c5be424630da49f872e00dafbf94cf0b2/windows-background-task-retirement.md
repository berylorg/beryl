# Reason For Investigation

A native Windows cleanup driver retained its service while suspended on a GPUI background
timer, although its authenticated source handoff had emptied the source list. Investigation
needed to distinguish timer progress, task cancellation, final future destruction, and safe
installation of an owned task handle.

# Outcome

At this revision, a nonzero `BackgroundExecutor::timer` schedules a separate empty future
through `PlatformDispatcher::dispatch_after`. On Windows, `ThreadPoolTimer::CreateTimer`
invokes its runnable. Completing that task wakes the awaiting driver; the driver's scheduling
closure then dispatches another runnable through
`ThreadPool::RunWithPriorityAsync(High)`. Timer completion and resuming the driver are separate
dispatches. Awaiting the timer suspends the future rather than blocking a pool worker.

A diagnostic remaining immediately before `timer.await` does not distinguish an unfinished
timer from a completed timer whose parent has not resumed. Continued foreground fixture
timers do not establish either stage's progress for this particular background task. The
investigation did not prove global timer shutdown, worker saturation, or dispatch refusal in
the observed stall.

GPUI's spawned `Task` owns an `async_task::Task`. Dropping it requests cancellation; detaching
it abandons cancellation ownership. In async-task 4.7.1, cancellation marks the task closed
and schedules destruction if it is neither running nor already scheduled. A running task
destroys its future when it next processes the closed state. Returning from handle drop is
therefore not proof that the future, its service Arc, or other retained resources are gone.
Actual future destruction and original resource uniqueness remain distinct completion
evidence.

`BackgroundExecutor::spawn_internal` schedules its runnable synchronously before returning
the handle. The Windows work-item handler captures the runnable. If scheduling refuses the
work and releases that handler, `Runnable::drop` synchronously calls `drop_future`. Thus a
future's ownership guard can run inside `spawn`, including before handle installation.
Holding a mutex across `spawn` deadlocks if that guard must reacquire the same mutex.

An owned-task starter can seal a run identity before spawning outside its locks, then install
the returned handle only if that identity is still current. The ownership guard must handle
destruction before installation without recursively retrying refused scheduling. A stop
accepted during startup must cancel the genuine returned handle when installation reaches
that sealed identity. A stale return must not replace a successor's handle. These are
dependency lifecycle findings; Beryl's authoritative documents control admission, handoff,
and recovery policy.

# Sources

- Canonical remote: `https://github.com/berylorg/zed-fork.git`; requested and Cargo.lock-resolved
  commit `edd4928c5be424630da49f872e00dafbf94cf0b2`. Accessed 2026-10-09 through focused inspection
  of Cargo's resolved primary sources for the native Windows dispatcher, with `rg`,
  `Get-Content`, and `Get-FileHash -Algorithm SHA256`; no dependency changes or Cargo runs.
- [`crates/gpui/src/executor.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/executor.rs):
  `Task`/`TaskState` and `detach`, lines 58–108; `spawn_internal`, lines 165–175; `timer`,
  lines 357–369. Inspected bytes SHA256
  `AA495D3A275635BFA99976C7FF7D17FFB10D80FDEC2119E0D3392001F67F608C`.
- [`crates/gpui/src/platform/windows/dispatcher.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/platform/windows/dispatcher.rs):
  captured runnable and immediate/background timer dispatch, lines 47–67. Inspected bytes
  SHA256 `F2BBBFD475C892D21E11D7E3EB9A8AD102CF9B974842425F8B73FD3321E47F10`.
- Cargo-resolved registry dependency: crates.io `async-task` 4.7.1, root `Cargo.lock`
  package checksum `8b75356056920673b02621b35afd0f7dda9306d03c79a30f5c56c44cf256e3de`.
  GPUI declares `async-task = "4.7"`, using its default `std` feature; the inspected paths
  concern production Windows scheduling, independently of simulated test scheduling.
- [`async-task` 4.7.1 `src/task.rs`](https://docs.rs/crate/async-task/4.7.1/source/src/task.rs):
  `set_canceled`, lines 183–229, and `Task::drop`, lines 440–445. Inspected bytes SHA256
  `8899DC897B21220A19134AE3755EEFEFEDA55F18A56E32145C2A97D69BE60FB6`.
- [`async-task` 4.7.1 `src/runnable.rs`](https://docs.rs/crate/async-task/4.7.1/source/src/runnable.rs):
  `Runnable::drop`, lines 893–932. Inspected bytes SHA256
  `E12BEC98BC1AA2F10194C85C7B022807CD5E1ABD95A37DB93BD5B06CED8D1A28`.
- [`async-task` 4.7.1 `src/raw.rs`](https://docs.rs/crate/async-task/4.7.1/source/src/raw.rs):
  ready-task destruction and awaiter wake, lines 562–607; closed running-task destruction,
  lines 612–654. Inspected bytes SHA256
  `5879172E761591FEE44293537ED134F794C42C873AFDD5CCB1107A8DC7CCCE6C`.
- Local use sites inspected: `crates/beryl-app/src/main_window/conversation_composer_owner/service.rs`
  cleanup-driver admission, timer loop, retained task, and ownership guard;
  `service/claim_cleanup.rs` authenticated source handoff and actual resource-drain diagnostics.
