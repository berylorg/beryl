# Transport and Admission

This supplement is normative only for its bounded backend transport-and-admission role and is governed by
[design.md](design.md). It does not independently declare engineering rigor.

## Managed Launch and Supervision

- Host Windows executes the caller-validated absolute Codex CLI path directly with `app-server`; it never substitutes `PATH`. WSL uses `wsl.exe`, the caller-validated distribution and working directory, and the caller-validated runtime-native CLI path directly inside WSL.
- Production uses only a Beryl-selected authenticated loopback WebSocket listener: host launch binds `ws://127.0.0.1:<port>` in the execution root; WSL binds in the selected distribution and uses the corresponding host-local loopback port. Production has no stdio, unauthenticated, or operator-managed transport.
- One high-entropy bearer token per managed launch exists only in memory and a per-run local token file. The crate creates and removes that file and clears retained token material on every spawn, admission, cancellation, exit, and disposal path; cleanup is idempotent and completes before managed-process disposal. Tokens never appear in arguments, logs, diagnostics, or public values.
- Token files are created with private access before any secret bytes are written: Windows uses
  a protected owner-only DACL instead of inheriting temporary-directory read access; Unix creates
  the file with mode `0600`. Windows verifies persistent ACL support on the exact opened file
  before writing secrets, refusing and cleaning up the empty file when enforcement is unavailable.
  Failed writes close the file before ordinary token cleanup.
- The managed server is the sole production connector constructor. It retains opaque per-launch provenance for exact runtime identity, process generation, executable paths, runtime mode, and working directory, and binds every session to that launch. Feature-gated test construction is not admission authority.
- Process lifetime is separate from client-session lifetime: closing a client never ends its managed server. Windows supervision covers the process tree; WSL has a Beryl-owned Linux cleanup boundary independent of the host `wsl.exe` wrapper. Shutdown is explicit, bounded, idempotent, and releases launch material only after its supervision boundary is released; it is never turn control or terminal evidence.

Managed server launch failures return the original typed error together with any unsettled
authentication, process and reader cleanup owner. Resource construction retains custody before
fallible post-spawn setup. Explicit cleanup is bounded and retryable on that same owner; a failed
reader join retains the original reader rather than reporting complete disposal. Consumers must
retain this failure capability and fence replacement until cleanup completes.

WSL supervision must retain authority over the original Linux workload through disposal. A saved
numeric PID or process-group ID is not durable incarnation proof and must not authorize a later
destructive signal after the original identity can have expired. Leader exit alone does not prove
whole-group closure. Absence must be distinguished from observation or permission failure; an
unproved boundary remains unavailable with original cleanup custody. The exact qualifying
mechanism and supported Linux environment must satisfy the system's
[native WSL privilege and proof contract](../../../doc/systems/backend-runtime/design.md#native-wsl-supervision-privileges-and-proof).
Namespace and Windows-job closure do not authorize unproved service-mediated cross-OS disposal.

## Fixed WSL Filesystem Observation

- The managed WSL path probe takes one exact distribution and a closed executable, directory or
  user-home operation. Selected paths are absolute distribution-native POSIX paths. It runs only
  the fixed observation program through the existing supervised `wsl.exe` launch boundary; no
  caller-supplied script, shell fragment, environment override or command is accepted.
- Results contain one canonical absolute native path of at most 4,096 UTF-8 bytes. The response
  protocol has a fixed tag and exact terminator; oversized, non-UTF-8, malformed or extra output
  fails closed. The reader retains at most 8,192 bytes, and stderr cannot block the helper on an
  undrained pipe. Executable observation verifies a readable executable file; directory and home
  observation verify a readable, traversable directory in the same distribution.
- Observation accepts a nonzero timeout of at most 30 seconds and cancellation. Success means
  complete validated output from the exact helper, not a live process or app-server admission.
  Explicit bounded shutdown joins the exact Linux supervision boundary, Windows launcher and reader on
  success, failure, cancellation and timeout. Failed shutdown retains its original owner for
  disposal; dropping a capability never grants a replacement probe or guessed cleanup success.
- These temporary helpers carry no backend bearer token, session, runtime-interest lease or
  release-admission provenance. Their observation facts cannot substitute for the production
  foreground release/profile/configuration proof. No helper targets another configured runtime
  process or an accepted turn.

## Release Admission and Session Profiles

- Every production launch supplies, in one atomic `SessionFlags` table override, `features.multi_agent_v2.enabled = true` and `features.multi_agent_v2.expose_spawn_agent_model_overrides = true`. A later scalar override, defaults, or command construction does not prove either fact.
- Admission requires four facts from one production foreground session: exact managed-launch provenance; observed initialize product token exactly `codex-cli 0.146.0`; immutable foreground profile selected before the first byte with every required notification enabled; and exactly one effective same-session `config/read`.
- The sole admission read must prove both required nested flags true with both dotted origins exactly `sessionFlags`. Missing, false, malformed, superseded, differently sourced, or detached facts fail closed. Admission performs no capability, model, private-steer, user-target, or synthetic-target probe.
- The retained admission report has only bounded product/version identity, exact opaque provenance, and required effective-configuration facts. Model discovery is a separate post-admission one-page query and never compatibility authority.
- Managed WebSocket is the sole production transport. Concurrent foreground and background work uses independent sessions. Each session owns immutable profile, initialize state, serialized request-id sequence, one non-cloneable exact response expectation, and bounded receive state. Request-only and foreground profiles are structurally distinct and cannot promote in place; uninitialized or request-only sessions never authorize foreground streaming.
- Foreground initialization sends the exact pinned experimental API and notification settings required for fields such as `agentNickname` and `thread/started`; refusal, mismatch, or opting out of required foreground `thread/started` fails initialization rather than selecting a fallback.

## Authenticated Transport, Responses, and Bounds

- WebSocket handshakes use `Authorization: Bearer <token>`. The transport owns authenticated handshake, framing, client masking, server-mask rejection, continuation and close state, protocol validation, bounded read-ahead and payload reads, and timeout behavior; it knows no JSON-RPC methods or normalized item types.
- Requests write through a fixed-capacity transport writer and fresh in-place client masks without a whole JSON string, request-sized byte vector, or masking copy. An outbound failure before any underlying byte may be accepted is nondispatch. A partial header, payload, newline, flush, or later failure is completion-unknown and closes the incomplete stream before reuse.
- A serialized wait binds exactly one request id and closed method-owned response family. The decoder constructs the typed result directly, discards incidental fields, and rejects or discards wrong, duplicate, reordered, missing, or trailing-mutated identities without a partial result. Initialization, config read, and model list each have their own closed result family.
- `JsonRpcError` exposes a finite code, bounded diagnostic text, truncation, raw-data presence, and only method-specific closed verdicts. Error text and raw data are never retry, lineage, dispatch, or history authority.
- Full-profile initialize exposes only the bounded app-server product/version token and required closed platform facts. Release-admission `config/read` is private and exposes only the two required flag/origin facts; every empty acknowledgement validates and discards its complete result object.
- The full profile selects its parser, page, prefix, queue, and ingress policy before its first transport read. It has no raw capture, whole-DOM, completion-time inspection, arbitrary field order, or non-target fallback; ambiguous or invalid pinned envelopes quarantine/discard or fail closed according to their exact family. Unknown notifications discard in order; unsupported server requests discard then fail the connection.
