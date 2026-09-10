# Reason For Investigation

Determine whether a small prestarted Windows reporter can retain a bounded panic record after the
application aborts, without inspecting or shutting down the failed application's shared state.
This was a documented-guarantee and local-use review on 2026-09-10, not runtime qualification.

# Outcome

A separate process can retain an anonymous pagefile mapping after the writer exits. Waiting on a
real synchronization-only process handle identifies that exact process object without a PID lookup.
Reading only after exit avoids simultaneous application writes during report validation. The
record still needs aligned ordered completion publication, a fixed extent and bounded length.

The receiver's logical read is not sufficient reason to map the page OS-read-only: Rust does not
guarantee acquire atomic loads on such pages. Use a writable receiver view for ordinary acquire
loads, or the specifically documented small relaxed load followed by an acquire fence. Review
caught this distinction before runtime acceptance.

Explicit inherited handles do not prevent inherited job membership. Breakaway may stop at an
ancestor job; the reporter must check that it is outside jobs before accepting readiness if it must
survive parent-owned kill-on-close containment. Environments that deny escape can decline reporting
without adding reparenting or launch brokers. External termination or later external job assignment
remains outside this startup guarantee.

The handle list requires real inheritable handles and enabled inheritance at process creation.
Duplicate the current-process pseudo handle into a real handle with only `SYNCHRONIZE`; close
temporary parent copies and clear child inheritance after transfer. A mapped view retains its
mapping, but the installed hook must not outlive its own view. Reporter presentation can occur only
after copying the bounded validated bytes into independent memory and releasing transport.

Rust's hook runs before stack unwinding, whereas `catch_unwind` observes the panic only after
intervening destructors have run. `abort` skips Rust destructors and buffered Rust I/O flushes.
The hook body can avoid application allocation, locks and waits, but Rust runtime payload/hook
handling occurs before it, mapped pages can fault, and lock-free atomics are not wait-free.
Consequently no end-to-end hard latency or universal-report guarantee follows from this design.

Local inspection found no existing explicit-inheritance mapping implementation to reuse. Existing
managed backend and diagnostic supervisors deliberately use kill-on-close jobs and are not a
reporter launch path. The workspace resolves `windows` 0.61.3; platform bindings and enabled
features still require focused compilation when implementation selects the exact API calls.

# Sources

The report-only clipboard path uses checked Windows calls: GPUI's ordinary clipboard write API
returns no success result. A non-null active owner window, movable UTF-16 allocation and explicit
successful ownership transfer avoid falsely acknowledging a failed write. GUI tests inject copy
success/failure and inspect full report export without replacing the Operator's clipboard;
native API ownership is covered by compilation and independent source review, not an OS clipboard
success test. [SetClipboardData](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setclipboarddata)
and [OpenClipboard](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard)
establish ownership transfer and null-owner/contended-clipboard failure behavior (accessed 2026-09-10).

Microsoft Learn API/process documentation, accessed 2026-09-10:

- [CreateFileMapping](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createfilemappinga):
  mapping references and cross-process coherence.
- [WaitForSingleObject](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject):
  process wait and required synchronization access.
- [Terminating a process](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process):
  process-object lifetime and child survival.
- [Nested jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs) and
  [job objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects):
  inherited chains, breakaway and kill-on-close consequences.
- [IsProcessInJob](https://learn.microsoft.com/en-us/windows/win32/api/jobapi/nf-jobapi-isprocessinjob):
  checking membership without assuming complete escape.
- [UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute):
  explicit inheritance-list requirements.
- [DuplicateHandle](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle):
  real reduced-rights handles.
- [Interlocked access](https://learn.microsoft.com/en-us/windows/win32/sync/interlocked-variable-access)
  and [InterlockedExchange](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nf-winnt-interlockedexchange):
  interprocess aligned atomic publication and ordering.
- [SuspendThread](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-suspendthread):
  debugger-oriented suspension and lock deadlock risk; it does not justify a live-GUI freeze scheme.

Rust standard-library documentation, accessed 2026-09-10:

- [catch_unwind](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html),
  [panic implementation](https://doc.rust-lang.org/src/std/panicking.rs.html),
  [atomics](https://doc.rust-lang.org/std/sync/atomic/index.html), and
  [abort](https://doc.rust-lang.org/std/process/fn.abort.html): hook ordering, runtime caveats,
  lock-free versus wait-free operations, and termination semantics.

Local use sites inspected at Beryl prerequisite `f46bcb78b71618e671e656d410f31663dfd9874a`:

- `crates/beryl-backend/src/managed_process.rs`: managed job lifetime.
- `crates/beryl-app/src/diagnostic_child_supervisor.rs`: diagnostic child containment.
- `Cargo.lock`: resolved Windows binding version.

# Refresh Triggers

The installed nextest 0.9.129 (commit `1358a22964df2595d30d830ad669b37093e4a732`)
[Windows runner](https://raw.githubusercontent.com/nextest-rs/nextest/1358a22964df2595d30d830ad669b37093e4a732/nextest-runner/src/runner/windows.rs)
permits breakaway from its own test job. Launching it through Cargo can still leave another job
in the inherited chain. Direct `cargo-nextest.exe nextest run` passed all 17 channel tests where
`cargo nextest` refused reporter readiness. Preserve the production membership check; see the
[containment lesson](../../../failures/crash-report-test-containment.md).

Recheck when the target platform, process containment, inherited-handle implementation, Rust
toolchain or Windows binding version changes, or when runtime qualification contradicts a claim.
