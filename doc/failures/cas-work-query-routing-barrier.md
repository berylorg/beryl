# Work Queries At Routing Test Barriers

A process-inventory test attempted to read connection work while the approval-install barrier
deliberately withheld the ingester acknowledgement. The test could not progress: the forwarding
sink holds its routing-state lock across synchronous submission and acknowledgement, and the
connection work source needs that lock to observe the current attachment. The test itself owned
the release needed by its blocked read. Durable and session revisions returned normally; tracing
isolated the wait to `connection_work_revision`.

This barrier is not a valid observation cut for the combined inventory. The existing source
contract permits serialization with an in-progress routing operation and does not promise a
nonblocking snapshot. The test now releases approval installation, waits at the independent stop
driver cleanup barrier, and observes permission custody before and after target loss. Both cuts
pass. Production source locking and execution behavior are unchanged.

When adding assertions to fault barriers, check whether the paused operation retains a lock used
by the observation API. Use a completed source transition or observe concurrently while another
test participant releases that transition. Keep the test job bounded so a mistaken cut cannot
leave children running. The guarded stalled jobs were terminated through their exact monitor
owners; final acceptance includes process and temporary-directory reclamation.

The owning regression is
[permission work](../../crates/beryl-app/tests/normal_terminal/permission_work.rs). No unresolved
source-contract change follows from this test correction.
