# Indexed Resource Pending After A Competing Frontier

The selected-history construction worker initially refreshed Pending only after its own clean
commit, exact reconciliation, or worker-capacity refusal. This assumed a definitive command refusal
meant no useful progress was possible.

Independent review exposed two windows observing the same absent or Building companion frontier.
One command commits; the other's original command refuses that stale frontier. Its fresh query can
still be Pending. Admitting that result without refresh suppresses later reads, so completion by
the peer never reaches that window.

The correction retains the original refusal and uses one bounded exact-build observation under
the existing worker and command permit. An appeared or strictly advanced Building frontier proves
peer progress and enables exact Pending refresh after measured publication and queue rotation.
Unchanged refusal and terminal states stay nonretrying. Only a fresh authenticated resource query
establishes Ready. No per-resource lock, queue, snapshot owner, or revision map is needed.

The deterministic `competing_missing_and_building_frontiers_preserve_refusal_and_report_peer_progress`
test forces both observation schedules and checks original NotCommitted transport and custody
release. The contract is recorded in [the app design](../../crates/beryl-app/doc/design.md).
