# Native Modal Foreground Dispatch

## Invalidated Assumption

On 2026-09-27, independent native-confirmation readiness review found that scheduling a modal
dialog outside GPUI entity borrows is insufficient to preserve foreground progress. The Windows
dispatcher at GPUI commit `19ef0796613226c123a3c9047865060e7639f713` uses
`for runnable in self.main_receiver.drain()`. Resolved flume 0.11.1 removes the entire queue into
that iterator. If dialog creation precedes cancellation in the same batch, the native modal call
holds cancellation in its outer stack; nested foreground messages find an empty receiver.

## Correction

The fork's native-modal progress contract requires pending tasks to remain reachable until they
execute. Phase 616 uses a finite entry-snapshot budget with one-at-a-time receive and verifies a later
already-queued task executes inside a nested native message pump before the first returns.
Native confirmation remains separately accepted in phase 615, including same-batch cancellation
and owner removal. This is a bounded prerequisite, with no change to shutdown product scope.
An unbounded receive loop is also invalid: self-waking tasks could keep it inside one native
message handler forever. A second native regression requires self-waking work to yield to close.

See [source investigation](../memory/topic/native-window-publication/owned-confirmation.md).
Native modal code must still avoid entity/state borrows across nested dispatch; fixing queue
access alone does not establish dialog identity, cancellation or destruction correctness.
