# Reason For Investigation

Beryl needed to know whether Codex App Server 0.137 exposed enforceable read-only controls per thread or turn, and whether subagent hierarchy had to be reconstructed only from collabAgentToolCall records.

# Outcome

Useful. The 0.137 schemas expose thread and turn sandbox policy primitives, including read-only, workspace-write, external-sandbox, and danger-full-access variants. The stable `ThreadStartParams` schema exposes `sandbox` as `read-only`, `workspace-write`, or `danger-full-access`; `ThreadForkParams` exposes the same field for forked threads; and `TurnStartParams` exposes `sandboxPolicy` for this turn and subsequent turns. The structured turn policy variants include `readOnly` with optional `networkAccess`, `workspaceWrite` with optional `writableRoots` and network flags, `externalSandbox`, and `dangerFullAccess`.

Experimental schemas add named `permissions` profile ids to `ThreadStartParams`, `ThreadForkParams`, `ThreadResumeParams`, `TurnStartParams`, and `ThreadSettingsUpdateParams`, plus `thread/settings/update.sandboxPolicy` for subsequent turns. These fields cannot be combined with the corresponding sandbox policy field on the same request.

The 0.137 `Thread` schema exposes nullable `parentThreadId` for subagent threads and optional display metadata such as `agentNickname` and `agentRole`. `collabAgentToolCall.senderThreadId` and `receiverThreadIds` should therefore be treated as live activity edges or fallback evidence rather than the only hierarchy source.

Some app-server process surfaces are outside ordinary thread sandbox inheritance. The inspected 0.137 schema does not expose a sandbox or permissions override inside the `collabAgentToolCall` item itself; `command/exec` is a standalone request with its own structured `sandboxPolicy`; `thread/shellCommand` is described as unsandboxed with full access; and experimental `process/spawn` is described as spawning a standalone process without a Codex sandbox on the app-server host.

Schema inspection did not prove runtime enforcement. A disposable live probe should verify that read-only thread or turn policy blocks filesystem writes before product policy depends on it.

# Sources

- Local codex-cli 0.137.0 generated stable app-server schema from codex app-server generate-json-schema --out <temp-dir>, accessed 2026-06-11.
- Local codex-cli 0.137.0 generated experimental app-server schema from codex app-server generate-json-schema --experimental --out <temp-dir>, accessed 2026-06-11.
- Legacy source: doc/research.md entry dated 2026-06-11.
- Legacy source: doc/app-server-contract.md, migrated on 2026-06-11.

# Local Integration Impact

- Hidden developer instructions are not an enforcement boundary.
- `turn/start.sandboxPolicy` and experimental `thread/settings/update.sandboxPolicy` are subsequent-turn controls; Beryl must not assume that a policy change can demote a turn that is already running.
- `approvalPolicy` and `approvalsReviewer` are separate from sandbox mode. A read-only or non-writer thread should still have approval routing behavior that prevents permission escalation from becoming an implicit write lease.
- Because `collabAgentToolCall` does not carry a sandbox override, read-only enforcement for CAS-spawned subagents must be verified through inheritance, thread metadata, or another app-server settings path before Beryl relies on it for a write-mutex product policy.

