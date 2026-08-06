# App-server Thread Lifecycle

## Bounded unload through `thread/unsubscribe`

The initial forked-child retention probe treated `thread/unsubscribe` as a bounded unload operation. It planned to accept `thread/closed` or `notLoaded` within 15 seconds before resuming the rolled-back child.

That approach is invalid for the exact `codex-cli 0.146.0` target. Its documented and tested contract removes only the caller's subscription, then retains a last-unsubscribed thread until it has had no subscribers and no activity for 30 minutes. The bounded probe would therefore time out without testing dynamic-tool retention.

The accepted course correction is to expose the target's explicit persistent-thread lifecycle operations in `beryl-backend`: archive for immediate removal from active session storage, unarchive for restoration, and delete for physical task-owned probe cleanup. The live retention probe must consume those typed operations rather than extending its wait, treating unsubscribe as unload, bypassing the backend boundary with raw requests, or leaving persistent probe threads without an exact report.

Affected authority and evidence:

- `doc/plan.md`, prerequisite thread-lifecycle phase and later live-retention phase.
- `doc/app-server-contract.md`.
- `doc/memory/topic/codex-app-server-0.146.0/forked-dynamic-tool-retention.md`.
- `crates/beryl-backend/tests/live_dynamic_tool_fork.rs`.
- OpenAI Codex tag `rust-v0.146.0`, app-server README and `thread_unsubscribe` integration tests.

The separate architecture gate subsequently passed: an exact-target live child turn proved that registered Beryl dynamic tools survive fork, full rollback, archive, unarchive, resume, and later use. The unsubscribe-as-unload approach remains invalid and must not be restored.

## Exact orphan identity after an ambiguously committed fork

The Phase 7 preparation plan initially required bounded timeout and cancellation coverage while also treating every `thread/fork` failure as proof that no child exists unless Beryl can report the orphan by exact child id.

That combined requirement cannot be implemented against the exact `codex-cli 0.146.0` app-server contract. A fork request can reach app-server and create a persistent child before the client times out, disconnects, or observes cancellation. If the response carrying the new thread id is then lost, Beryl has no idempotency key, transaction handle, or request-to-child correlation API with which to identify or delete the committed child. Bounded thread inventory comparison is stale and non-unique, so it cannot establish exact identity and is already forbidden as a lineage or identity substitute.

Implementation stopped before Phase 7 code was written. Operator accepted an explicit indeterminate fork outcome as the course correction: Beryl keeps the source selected and idle, warns that an unidentified backend child may exist, refreshes inventory, and never guesses or activates a candidate. Cancellation remains cooperative before dispatch and between completed requests rather than forcing an in-flight fork connection closed and manufacturing false certainty. No inventory heuristic or GUI-local correlation metadata is an accepted correction.

Affected authority and evidence:

- `doc/plan.md`, Phase 7 blocker and timeout/cancellation acceptance cases.
- `doc/features/lifecycle-yield/design.md`, serialized handoff and preparation-failure semantics.
- `crates/beryl-app/doc/design.md`, bounded and cancellable background preparation.
- `crates/beryl-backend/src/session.rs`, request timeout and generic fork error boundary.
- `crates/beryl-backend/src/thread_branch.rs`, fork response identity contract.
- `crates/beryl-backend/tests/launch_and_protocol.rs`, exact fork wire contract without idempotency or correlation metadata.

## Bounded restart inventory with a large global thread store

The Phase 10 live acceptance initially assumed that restarting an isolated Beryl home would let the GUI rediscover and reactivate its two newly created phase children through the ordinary bounded member-thread inventory.

That approach is invalid in the current machine environment. The freshly built Beryl GUI completed the exact root-to-two-siblings lifecycle against `codex-cli 0.146.0`, and both backend rollout records retained the original root as `forked_from_id`. After restart, however, each ordinary `thread/list` inventory request exceeded Beryl's 10-second deadline against the machine's large global CAS thread store. The bounded selector therefore remained empty and rejected exact child activation as unknown, even though the isolated Beryl database still retained both child registrations and their original `orchestration_root_thread_id`.

Do not reinterpret the timeout as missing backend lineage, extend the deadline locally, infer children from an inventory delta, or add GUI-local substitute lineage. The accepted live evidence proves repeated clean sibling creation, dynamic-tool retention, persisted backend parentage, persisted Beryl orchestration provenance, and no final extra child.

The accepted architectural correction separates exact startup recovery from selector inventory. An in-scope persisted active registration may seed one exact backend activation, but only after the backend validates the requested id, execution target, canonical working directory, member binding, bounded history, and expected direct fork parent. This does not add the registration to selector inventory or manufacture lineage. Ordinary startup discovery and member inventory instead use backend CWD filters plus aggregate elapsed-time, page, result, and metadata-read budgets, and they expose incomplete or failed coverage without guessing.

The corrected restart acceptance passed on August 4, 2026. The freshly built GUI selected persisted child `019fce4c-95d1-7a12-b019-c3bc9de748bb` directly and became idle with its transcript loaded while the independent member inventory `thread/list` request timed out after approximately 10 seconds and retained zero selector rows. This proves exact recovery is independent of the machine's large global thread store and that inventory failure neither erases nor substitutes the validated active registration.

Affected authority and evidence:

- `doc/plan.md`, Phase 10 restart/reload acceptance boundary.
- `doc/features/conversation-threads/design.md`, backend-owned lineage and inventory behavior.
- `doc/memory/topic/codex-app-server-0.146.0/forked-dynamic-tool-retention.md`, exact live identities and retained proof.
- `crates/beryl-app/src/shell/member_thread_inventory.rs`, bounded GUI inventory owner.

## Invalid persisted identity falling through to discovery

The first Phase 10 startup implementation represented an ineligible persisted active registration by returning no exact recovery target. Startup interpreted that absence as permission to open the primary member through ordinary bounded discovery. Because the restore-preferred policy may select the newest available backend thread when its preferred id is absent, a rebind-required or binding-mismatched persisted identity could silently select an unrelated substitute.

That representation is invalid. "No persisted active identity exists" and "a persisted active identity exists but cannot be validated" are different lifecycle states. The corrected startup router has three explicit branches: exact activation for an eligible persisted registration, repair-required for an invalid persisted registration, and bounded discovery only when no persisted active registration exists. The repair-required branch performs neither `thread/list` nor activation, preserves the persisted identity, and presents a bounded repair notice.

Deterministic production-adapter tests now count list and activation dispatch for all three branches. A fresh completion review found no remaining substitution path. Do not collapse repair-required back into an optional exact target or interpret failed eligibility as permission to discover another thread.
