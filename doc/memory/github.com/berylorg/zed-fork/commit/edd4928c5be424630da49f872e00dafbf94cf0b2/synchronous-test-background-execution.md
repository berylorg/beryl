# Reason For Investigation

Mounted New Thread uncertainty fixtures held an actual reconciliation fault barrier while
waiting for the virtual GUI to observe it. Both cases reached the test timeout because the
background worker blocked the same thread that pumps the simulated GUI.

# Outcome

At this revision, GPUI's test background executor simulates concurrency by scheduling tasks
one at a time. `TestDispatcher::tick` invokes the chosen runnable synchronously.
`TestAppContext::run_until_parked` uses that executor; it cannot return to a reached-barrier
predicate while a runnable is blocked inside a synchronous fault.

A fixture requiring a held real worker must explicitly opt into actual concurrency. Beryl's
bounded adapter runs the same tracked original reconciliation worker on one real thread and
awaits its unchanged completion through the existing task. The adapter defaults off, retains
the original worker witnesses, and joins only after completion. It changes neither production
scheduling nor original admission and outcome semantics. A cancelled awaiting task must not
join a worker while the fixture still holds its fault barrier.

Refresh this finding when the GPUI pin, dispatcher or fixture scheduling changes. Increasing
the timeout cannot resolve this synchronous dependency.

# Sources

- Canonical remote: `https://github.com/berylorg/zed-fork.git`; full resolved commit:
  `edd4928c5be424630da49f872e00dafbf94cf0b2`, selected by the Beryl root `Cargo.toml`.
  Accessed 2026-10-08 by focused source inspection for Windows virtual-GPUI fixtures.
- [`crates/gpui/src/executor.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/executor.rs):
  `BackgroundExecutor` distinguishes the production pool from simulated test scheduling.
- [`crates/gpui/src/platform/test/dispatcher.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/platform/test/dispatcher.rs):
  `TestDispatcher::tick` calls the chosen runnable synchronously.
- [`crates/gpui/src/app/test_context.rs`](https://github.com/berylorg/zed-fork/blob/edd4928c5be424630da49f872e00dafbf94cf0b2/crates/gpui/src/app/test_context.rs):
  `TestAppContext::run_until_parked` delegates to its background executor.
