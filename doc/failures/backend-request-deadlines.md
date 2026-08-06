# Backend Request Deadlines

## Relative rebasing and post-error clock sampling did not preserve the deadline contract

Phase 16 full-suite verification exposed a bounded thread-list failure under scheduling load. The first correction converted selected transport errors to `RequestTimeout` when the clock was at or beyond the request deadline. Focused tests passed, but Deep completion review invalidated that approach.

Bounded pagination computed a remaining duration from its aggregate deadline and later let `request_json` create a fresh deadline from that stale duration. Preemption between those steps could extend the collection past its original absolute bound. The correction also sampled time only after a transport error returned, so a genuine pre-deadline close or protocol failure could be reclassified if scheduling crossed the deadline before classification. Immediate-close coverage did not exercise that boundary.

The same review exposed a broader contract gap: stdio request writes use blocking `write_all` and `flush` without the established deadline. Portable `ChildStdin` has no write-timeout operation, and a detached timeout wrapper would allow a partial or late JSON request to commit after the caller returned.

The first supervised correction also treated WebSocket deadline expiry as reusable and used the ordinary close-frame path when poisoning a connection. That was invalid once a frame header or payload could already be partially committed: the stream was no longer safe for another request, and writing a close frame on the same blocked socket could itself exceed the request boundary. Direct TCP abort replaced that immediate close-frame write, but final review still found pre-write expiry misclassified as possibly partial, write-expiry provenance lost in streaming control-frame handling, and normal Drop attempting a close frame after abort. The final transport must distinguish ordinary read or pre-write expiry from a possibly partial WebSocket write and must never issue another protocol write after terminal abort.

Fresh completion review exposed a limit in the recommended portable writer architecture. Killing the owned child normally releases the unread pipe and lets retained cleanup join the writer. If the operating system rejects every exact-child, process-tree, and runtime-boundary termination attempt, however, portable blocking `ChildStdin` offers no independent cancellation operation. Unconditionally joining then makes final cleanup unbounded; returning without the join detaches or leaks ownership. The three guarantees—bounded final cleanup after termination failure, no detached writer, and portable blocking stdio—cannot all be retained without either a documented exceptional failure contract or platform-native cancellation.

The retained implementation direction still requires one unchanged absolute deadline, typed internal deadline-expired outcomes with write-commit provenance, and a supervised writer that owns stdin. A timed-out stdio write is session-fatal: the owned child is terminated to unblock the pipe, writer and process cleanup remain owned and joinable on the successful termination path, and later requests are rejected. A possibly partial WebSocket write is likewise terminal for that client connection but does not transfer or terminate separately owned managed-server process state. Operator resolved the exceptional failure contract: explicit shutdown returns a bounded retryable failure and retains ownership when termination fails; normal cleanup joins both resources; final Drop may block after repeated operating-system termination refusal rather than detach the writer. Platform-native cancellation remains out of scope. The implementation must not weaken this with relative rebasing, post-error clock inference, detached workers, unbounded close writes, or timing-only tests.

The completed correction implements that contract. Cleanup returns the exact process and writer to a retained retry state after bounded termination refusal, and final Drop keeps the supervisor live while joining rather than destroying process ownership first. Stdio and WebSocket writes both track byte commitment for timeout and ordinary I/O failures; WebSocket streaming-control errors preserve the same provenance. Deterministic tests cover pre-byte reuse, post-byte terminal behavior, request-id allocation, raw abort without a close frame, retained cleanup retry, exact failure aggregation, child reclamation, and a fragment/Ping schedule whose valid late response distinguishes the original deadline from a reset deadline.

A payload-size-only loopback fixture was rejected for partial WebSocket write coverage because kernel buffering varies by platform and does not prove which frame boundary was committed. The deterministic fixture instead pauses after a complete frame header under test-only instrumentation, then crosses the unchanged deadline before allowing the payload path to continue.

Affected authority and evidence:

- `crates/beryl-backend/doc/design.md`.
- `crates/beryl-backend/src/session.rs`.
- `crates/beryl-backend/tests/bounded_thread_list.rs`.
- Root `doc/plan.md`, Phase 17.
