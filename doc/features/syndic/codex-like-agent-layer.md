# Syndic Codex-Like Agent Layer

This supplemental feature doc captures the constraint baseline for a Codex-derived or Codex-like local agent layer that Beryl could talk to instead of stock Codex App Server.

It is an engineering checklist, not legal advice. It is authoritative for the constraints such a layer must preserve before adding Beryl-specific storage, projection, media, scheduling, or protocol improvements, but it is not an implementation plan and does not authorize replacing Codex App Server by itself.

# Non-Negotiable Functionality For Compliance

## Official Auth Modes

Support the same documented auth classes Codex supports: ChatGPT sign-in for subscription access, API key auth for usage-based access, and Enterprise Codex access tokens where applicable.

Do not create a generic ChatGPT OAuth client outside Codex or treat ChatGPT subscription auth as a general OpenAI API billing layer.

## Per-User Credentials

Each Beryl user must sign in with their own account or API key.

Do not pool subscriptions, rotate accounts, proxy many users through one user's token, or share cached credentials across users.

## Codex-Compatible Token Lifecycle

Preserve Codex-compatible auth cache and refresh behavior.

Handle logout, token revocation, session expiry, `401` refresh/retry behavior, and secure local credential storage. Treat auth cache files as password-equivalent secrets.

## Managed Configuration

Honor managed/admin configuration such as `forced_login_method`, `forced_chatgpt_workspace_id`, credential-store configuration, custom CA configuration, and comparable policy-bearing settings.

If credentials or runtime setup violate managed policy, fail closed.

## Workspace And Plan Policy

Preserve workspace identity, role-based access, feature gates, model availability, retention implications, and residency implications returned by the OpenAI service.

If a model, capability, or workspace feature is unavailable, surface that result instead of working around it through lower-level calls.

## Usage Limits

Respect rate limits, quota limits, plan limits, overload responses, and feature-unavailable responses.

Use bounded queues, cancellation, and backoff with jitter. Do not retry, shard, parallelize, or rotate identities in order to evade limits.

## No Private Bypasses

Do not call undocumented or private endpoints except through the Codex code path being forked and preserved.

Do not replay Codex or ChatGPT tokens against unrelated ChatGPT or OpenAI internals to bypass Codex restrictions.

## CAS-Like Session Semantics

Provide a session model with request/response/notification behavior equivalent to the CAS contract Beryl depends on.

This includes initialization, shutdown, request IDs, structured errors, cancellation, server events, and bounded ingress/egress queues.

## Thread Lifecycle

Implement durable thread operations equivalent to CAS behavior.

Required operations include thread start, resume, read, list, fork, rollback, archive, unarchive, delete, name update, exact thread IDs, parent metadata, fork metadata, and durable thread state.

## Turn Lifecycle

Implement turn operations and state transitions equivalent to CAS behavior.

Required behavior includes turn start, steering, interrupt, idle/running status, completion, failure, abort, compaction, token usage, and final response extraction.

## Canonical Replayable History

Maintain a canonical replayable history equivalent to Codex rollout semantics.

Custom storage may become the efficient read path, but it must not break resume, fork, rollback, compaction, or model-context reconstruction.

## Lazy Projection Layer

Treat efficient Beryl-facing reads as projections over canonical history.

Projection support should include cursor-walked turns, cursor-walked items, generated-image byte redaction, media path indexing, and correct rebuild or invalidation when canonical history changes.

## Event Projection

Preserve all streamed item classes Beryl expects.

This includes user messages, assistant messages, reasoning events according to visibility policy, shell commands, file changes, patch approvals, MCP calls, dynamic tools, generated images, subagent events, errors, and token counts.

## Sandbox Model

Preserve Codex sandbox behavior for read-only, workspace-write, network access, writable roots, WSL/native execution boundaries, and unsupported policy refusal.

The agent layer must not silently broaden filesystem, process, or network access.

## Approval Model

Preserve approval policies, approval scopes, approval prompts, denial behavior, and per-tool approval annotations.

Relevant policy modes include on-request style approvals, untrusted operation handling, and never-ask modes.

## Subagent Policy Inheritance

Subagents must inherit sandbox and approval policy from the parent execution context unless a documented Codex policy explicitly allows narrowing or changing it.

Subagent approval requests and events must preserve source thread and agent identity.

## Execution Controls

Implement soft stop, hard stop, background terminal tracking, command termination, cleanup, and process reaping.

The runtime must not leave orphaned privileged processes after stop, shutdown, or thread closure.

## Remote Listener Security

Default to local-only connectivity.

If any WebSocket or remote-control listener is exposed, require capability-token or signed-bearer authentication, reject unsafe origins, and keep all queues and message sizes bounded.

## Secret Hygiene

Redact tokens, API keys, bearer headers, cookies, and credentials from logs, diagnostics, traces, events, errors, and transcript projections.

Do not include secrets in persisted projection rows or debug exports.

## Auth-Mode Data Boundaries

Keep ChatGPT-managed Codex auth separate from API-key auth.

Do not silently move a user from one auth mode, workspace, org, or billing context to another, because that can change data handling, retention, and policy behavior.

## User-Controlled Persistence

Honor transcript and history settings, archive/delete semantics, generated media paths, and local retention controls.

Do not silently copy workspace data, transcript history, or generated artifacts into a new durable store without a clear local policy.

## External Data Exfiltration

Preserve network restrictions and prompt-injection defenses around repository contents, secrets, logs, generated artifacts, and other workspace data.

Do not send additional data to external services merely because the custom layer has easier access to it.

## License And Notices

If distributing a Codex fork or derived runtime, preserve the Apache-2.0 license obligations and third-party notices from the Codex repository.

Track any additional dependencies introduced by the custom layer.

## Branding Clarity

Do not present a fork or derived runtime as official OpenAI Codex.

Name and describe the runtime clearly as a fork, derivative, or compatible agent layer.

## Version Compatibility

Track the upstream Codex commit and protocol version the runtime targets.

Fail clearly when Beryl connects to an incompatible runtime rather than silently degrading policy, auth, history, or safety behavior.
