# Manual Compaction Feedback

## Settlement And Retirement Share One Fence

Publishing manual feedback after durable settlement outside the coordinator's settlement fence
allowed concurrent shutdown to publish authority loss first. A committed exact success could then
be hidden by the sticky loss result. Manual settlement now holds the same fence through durable
settlement, local removal and exact feedback publication; retirement resolves remaining feedback
under that fence. The native Compact regression pauses after settlement and starts retirement,
proving retirement waits and the original consumer receives success after local removal.

## FIFO Includes Pending Handoffs

Sorting only arrived feedback allowed a newer Compact result to occupy the protected Notice while
an earlier Stop handoff was still pending. The shared presentation order now includes pending
handoffs, and the first unresolved entry determines the sole protected contribution. Native
qualification delays the earlier Stop arrival, saturates ordinary notices, then verifies Stop
appears first and dismissal reveals the persistent Compact result.

## Fixtures Must Respect Original Custody And Clock

The lifecycle admission fixture attempted process reopen while ordinary execution still held its
original counted custody. `Unsettled` is required there; reopen succeeds after cleanup. Pause
controllers must live inside the thread scope so assertion unwind releases paused workers before
joining them. The analogous manual fixture has no ordinary execution custody and retains its
successful early-reopen assertion.

Native composer acceptance also uses current wall time. A fixed test timestamp older than the
genuine mounted save produced `TimestampRegressed`; advance the fixture clock past the genuine
timestamp before accepting queued input. Neither correction changes production admission rules.

The owning contracts remain App live control, CAS-live context compaction and Notifications.
These lessons apply to the mounted manual command and its acceptance fixtures.
