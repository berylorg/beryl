# Transport and Admission

This supplement is normative only for its bounded backend transport-and-admission role and is governed by
[design.md](design.md). It does not independently declare engineering rigor.

## Managed Launch and Supervision

- Host Windows executes the caller-validated absolute Codex CLI path directly with `app-server`; it never substitutes `PATH`. WSL uses `wsl.exe`, the caller-validated distribution and working directory, and the caller-validated runtime-native CLI path directly inside WSL.
- Production uses only a Beryl-selected authenticated loopback WebSocket listener: host launch binds `ws://127.0.0.1:<port>` in the execution root; WSL binds in the selected distribution and uses the corresponding host-local loopback port. Production has no stdio, unauthenticated, or operator-managed transport.
- One high-entropy bearer token per managed launch exists only in memory and a per-run local token file. The crate creates and removes that file and clears retained token material on every spawn, admission, cancellation, exit, and disposal path; cleanup is idempotent and completes before managed-process disposal. Tokens never appear in arguments, logs, diagnostics, or public values.
- The managed server is the sole production connector constructor. It retains opaque per-launch provenance for exact runtime identity, process generation, executable paths, runtime mode, and working directory, and binds every session to that launch. Feature-gated test construction is not admission authority.
- Process lifetime is separate from client-session lifetime: closing a client never ends its managed server. Windows supervision covers the process tree; WSL has a Beryl-owned Linux cleanup boundary independent of the host `wsl.exe` wrapper. Shutdown is explicit, bounded, idempotent, and releases launch material only after its supervision boundary is released; it is never turn control or terminal evidence.

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
