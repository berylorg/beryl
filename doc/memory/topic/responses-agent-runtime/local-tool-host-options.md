# Reason For Investigation

Replacing CAS transfers shell, file, policy and model-facing orchestration responsibilities to
Beryl. Determine which Rust components can be reused and which boundaries remain new work.
This is research under the implementation hold, not an approved execution architecture.

# Outcome

Selected Rust reuse is plausible, but replacing CAS with an HTTP client does not replace its
tool host. The largest additional choices are code-mode execution, process supervision and
sandbox policy across native hosts and WSL. Dropping enterprise accounts removes none of these
local execution requirements. No sandbox, tool-quality or resource-bound experiment was run.

## Code Mode Is A Separate Runtime

The directly queried Luna catalog selects `code_mode_only` and Responses Lite. Small direct
ordinary-function requests nevertheless succeeded. This establishes service acceptance for those
requests, not equivalent coding quality or permission to silently ignore model-client guidance.

Pinned Codex uses a Rust-hosted V8 runtime, version `150.4.0` with V8 sandbox support and ICU,
rather than requiring Node. A session delegate routes nested tool calls back to the host. Each
call gets an isolate/module; imports are rejected. The environment provides tool bindings, output
helpers, timers, retained values and yielding, while removing globals including console,
Atomics, SharedArrayBuffer and WebAssembly. It orchestrates tools; it does not replace shell or
filesystem enforcement. The client fails closed when a required code-mode host is unavailable.

An important reuse limitation is that `InProcessCodeModeSession::with_delegate_and_limits`
overwrites `max_heap_size_bytes` with `None`. The inspected implementation uses default isolate
creation, an unbounded event channel and retained JSON maps. Separate request/cell caps do not
establish a heap bound. Importing this runtime unchanged cannot be credited as satisfying Beryl's
bounded-memory requirements. Root independently checked the constructor; the source investigator
traced the other runtime paths. No claim is made about measured memory growth or V8 security.

An out-of-process host could provide a termination boundary, but memory limits, output
backpressure, nested effects and restart semantics still need explicit design and verification.
It must not turn into a replacement general-purpose app server merely to avoid making ownership
decisions. Plain function tools remain an alternative only where the selected model's required
behavior and task quality are established.

## Process And File Components

`codex-utils-pty` is a comparatively narrow reuse candidate for PTY/pipes, stdin, resizing, exit
and termination. It uses Tokio and portable-pty, with Unix process groups and Windows ConPTY/job
handling. It does not provide Beryl's permission policy, durable invocation identities or result
custody. High-level unified execution is much more coupled to sessions, turns, approvals,
network policy, plugins, metrics and events. Its source has a 1 MiB head/tail output limit and
64-process limit; these are client policy values, not selected Beryl requirements.

Shell selection, login behavior, environment inheritance, PowerShell encoding and permissions for
subsequent stdin writes are observable behavior. A yielded terminal is an ongoing effect, not a
completed command. Reconnection cannot silently recreate or rerun it. Existing Beryl managed
process-tree disposal is reusable lifecycle knowledge, but currently owns CAS disposal and does
not by itself implement per-tool cancellation.

`codex-apply-patch` depends on `codex-exec-server`; it is not merely a small patch parser to import.
Root independently verified this dependency. Its filesystem abstraction reads whole UTF-8 files
and constructs whole replacement content. Default behavior follows symlinks and normalizes LF,
with a line-ending-preservation option. Multi-file application is sequential and can report an
already-applied prefix after failure. Therefore reuse needs explicit file-size/encoding policy,
path enforcement, concurrent-edit detection and honest partial-effect reporting. It cannot be
described as atomic multi-file editing or bounded streaming for arbitrarily large files.

`codex-file-search` provides filename fuzzy search using ignore/nucleo. It is not a substitute
for content search; rg or another deliberate content-search implementation remains separate.

## Permission And Sandbox Boundaries

Exec policy evaluates Starlark prefix rules for allow/prompt/forbidden decisions. That is distinct
from OS containment. Likewise an approval cache is not a sandbox, and a previously admitted
terminal may need renewed checking when later stdin or filesystem policy changes its risk.
Preserve Beryl V1 automatic approval denial unless Operator-owned design explicitly changes it.
This investigation does not add an approval UI or persistent grants.

Linux execution uses bubblewrap, filesystem mounts/namespaces and seccomp. Source supports WSL2
and explicitly rejects the inspected WSL1 route. macOS Seatbelt and Windows restricted-token or
elevated backends have different helpers and policy behavior. Packaging, ACL setup and helper
lifecycle are substantial work; testing native Windows cannot establish WSL enforcement.

Filesystem tools also need enforcement. The inspected execution server can open files through a
sandboxed helper and transfer handles/descriptors. Merely canonicalizing a path before an
unrestricted open does not prove protection against a symlink race. A standalone patcher that
bypasses the execution policy would widen permissions despite a correctly sandboxed shell.

## Candidate Ownership Choices

- **Beryl host with selected Rust components:** strongest direct control of admission, effects,
  storage and limits, but requires assembling process and sandbox integration and maintaining it.
- **Dedicated Codex-derived execution helper:** reuse selected execution-server and sandbox
  machinery outside Beryl's GUI process, without CAS. This imports protocol, configuration,
  packaging and upgrade coupling that must be assessed explicitly.
- **Plain tools before code mode:** smaller initial runtime only if the chosen model's behavior
  and quality justify it. Direct acceptance of a tiny function call is insufficient evidence.

All options need durable invocation/result custody from the
[execution assessment](durable-execution-and-recovery.md), bounded output retention and a policy
for unknown effects after crash. Whole-file patch memory and JavaScript heap behavior are separate
from the network ordering question; they are not evidence that subscription streaming forces
identity-related disk buffering.

## Remaining Decisions And Verification

Preserve exact working-root and host/WSL identity from the
[responsibility inventory](cas-responsibility-inventory.md); do not route execution to a convenient
different environment. Time-to-first-yield, execution deadline and cancellation/settlement timeout
are distinct policies still to select. An expired approval obligation cannot become permission:
V1 denial means no imported sticky/session grant cache or persistent approvals. Review accepted
these source findings with runtime/sandbox verification explicitly outstanding.

Select required tool behavior before choosing dependencies. Verify process-tree exit and bounded
output under native/WSL execution, stdin-after-yield policy, cancellation during file/network
effects, concurrent edits, symlink races and partial patch failure. Verify code-mode heap/output
limits and nested-call cancellation if adopted. These are local deterministic or isolated tool
tests, not reasons for additional subscription quota consumption at this stage.

The workspace is Apache-2.0 with attribution/NOTICE obligations, but this inspection does not
clear every transitive/native dependency or V8 redistribution requirement. Packaging and license
review belongs to the operations assessment. No dependency or executable was installed.

# Sources

Source-only inspection on 2026-09-17 at Codex commit
`6b9826e3aa83b1a5947db50f4332cb9c65f1b340`:

- [Runtime dependencies](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/code-mode-runtime/Cargo.toml),
  [session limits](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/code-mode-runtime/src/service.rs),
  [runtime](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/code-mode-runtime/src/runtime)
  and [host](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/code-mode-host/src/lib.rs).
- [PTY](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/utils/pty),
  [unified execution](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/unified_exec)
  and [patch dependencies](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/apply-patch/Cargo.toml).
- [Patch implementation](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/apply-patch/src/lib.rs),
  [filesystem](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/file-system)
  and [filename search](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/file-search).
- [Linux sandbox](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/linux-sandbox/README.md),
  [sandbox selection](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/sandboxing/src),
  [sandboxed file opening](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/exec-server/src/sandboxed_file_open.rs)
  and [exec policy](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/execpolicy/README.md).
- [Direct model and compaction evidence](subscription-streaming-compaction.md),
  [responsibility inventory](cas-responsibility-inventory.md), and local
  `crates/beryl-backend/src/managed_process.rs` for existing process ownership.
