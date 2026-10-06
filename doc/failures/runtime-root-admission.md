# Runtime And Root Admission

## WSL Probe Ownership Readiness

On 2026-10-06, phase 733 exposed a prerequisite omitted from the
[mounting-readiness evidence](../audits/runtime-root-creation-readiness.md): exact WSL executable,
directory and user-home observation requires a supervised filesystem helper before managed
foreground release admission can construct its working-directory binding.

The [backend package boundary](../../crates/beryl-backend/doc/design.md#package-boundary) currently
constructs and supervises Beryl-owned `app-server` processes. Its reusable
`SupervisedBackendProcess` and `WslProcessGroupCleanup` are private implementation types in
`managed_process.rs` and `command.rs`. The public
[ManagedBackendServer](../../crates/beryl-backend/src/server.rs) accepts a managed app-server launch
specification; it cannot execute an exact filesystem probe. Killing only the Windows `wsl.exe`
launcher would not establish Linux process-tree disposal.

The attempted composition added a public, fixed WSL path-probe capability to that backend package.
Although it reused the existing supervisor, this introduced a filesystem-helper process boundary
without owning package/system authority. The earlier readiness record did not settle that
responsibility split. No probe, backend process, native GUI, clipboard access or Cargo test ran.

The Operator resolved the decision on 2026-10-06 by instructing continuation through Beryl's
existing `wsl.exe` path. The backend-runtime system and backend package/transport authority now
assign the fixed helper to the existing backend supervisor, retaining application-owned
path/environment admission and defining fixed inputs, bounded output, cancellation, timeout and
joined cleanup custody. Resume implementation under that corrected authority; the helper and
atomic admission still require their own behavioral qualification and independent review.

The resumed unaccepted draft is retained locally at `.tmp/runtime-root-admission-resumed-draft`
(43 source/manifest files plus patch, path manifests, SHA-256 inventory and resumption note; under
2 MiB). Root owns this exact scope until readiness is resolved and the draft is reconsidered or
discarded. It is excluded from production source and version control. The superseded seven-file
archive was reclaimed after the complete replacement archive was verified.

Independent source/authority review confirmed the gap and accepted this blocker record with no
blocking documentation findings. During the pause, the seven touched tracked source files were
reverted and all six new production source drafts were removed. Heading spacing and scoped
whitespace checks passed; documentation reconciliation returned Current with zero failed or
blocked chunks. The draft was restored for implementation after the Operator resolved ownership.

## Supervision Cleanup Proof

Review of the resumed implementation found that reusing the existing WSL supervisor did not
itself prove complete cleanup. An absent PID file returned a negative observation that callers
discarded; killing or observing exit of the Windows launcher could then be reported as successful
Linux process-group disposal. Post-spawn initialization failures also discarded their process
owner, and joining the stderr reader had no deadline.

The correction retains the original process, authentication material and reader through failed
cleanup. Fixed filesystem helpers require evidence from their own process-group or completion
marker; absent or delayed evidence remains unavailable. Cleanup helpers retain their exact child
until joined, and retries use the same original owner. Managed admission and the existing runtime
interest owner must preserve that custody, fence replacement and report incomplete disposal.
Retaining a failed runtime inside its interest owner is insufficient when consuming service close
then drops that owner. The same disposal-only capability must survive ordinary close, failed-home
retirement and their process-owner failure slots, with retained home custody and replacement
fences. Tests must exercise these consuming paths rather than only retrying a retained harness.
The local app run `14b12841-c282-4c9f-a6f0-ef149f5d9213` passed 40 atomic, injected validation and
runtime-interest cases. Canonical backend run `4591f5d7-bcf9-49fe-aee4-23981986facc` passed 16 selected
command/probe/cleanup cases, with three real managed-launch cases excluded. Source review found
the retained launch-failure and bounded stderr-join corrections sound. Later token clearing and
consuming service-close/recovery corrections were not fully qualified; their disposal retry still
needs its production caller. No native Beryl GUI, Operator clipboard or backend/WSL helper ran.
These subsets do not accept the final archived source or actual WSL environment behavior.

## Exact Linux Group Ownership

Further review invalidated the shared shutdown approach. The draft retained a numeric group ID
after its original leader exited. A delayed cleanup retry could signal an unrelated group that
reused that number. Linux removes released numeric identities and allocates them cyclically;
this is mechanism evidence, not qualification of the Operator's WSL kernel.
[Linux PID implementation](https://github.com/torvalds/linux/blob/master/kernel/pid.c)

The fixed helper's normal completion marker does not cover absent-marker paths or shared
app-server cleanup. An original live leader handling a nonce shutdown request and signalling its
own current group would avoid destructive signalling through an expired saved ID. Its exit alone
does not prove all group members stopped, and shell `kill -0` failure cannot distinguish absence
from permission failure. No qualifying existing-utility alternative has been demonstrated;
utilities are not categorically ruled out.

The recommended next step is an explicitly designed narrow Linux-side supervision owner, retaining
the existing exact-distribution `wsl.exe` path, stable original identity and typed no-start,
termination and permission-failure evidence. Its artifact/deployment requirements and supported
kernel primitives need owning authority before implementation. A pidfd alone does not prove
whole-group or descendant closure. The Operator approved investigation and then the privileged
supervisor/bootstrap boundary on 2026-10-07; no companion is built, installed or qualified.

Implementation paused under the Operator's technical-plan-failure rule. All 25 touched tracked
source/manifest paths were restored and all 18 new source files moved into the verified archive.
The isolated canonical checkout and exact worktree registration were reclaimed. Restored locked
metadata and focused app library check passed, followed by successful language-server restart.
Phase 733 remains unaccepted and blocked on the supervision proof mechanism.

Independent review accepted this documentation-only blocker handoff with no blocking findings and
verified all 43 retained snapshots against their SHA-256/size inventory. The complete archive is
556,231 bytes. This accepts the record and safe pause, not the implementation or proposed companion.

## Cross-OS Ownership Outside The Namespace

The privileged namespace candidate establishes a Linux closure mechanism, not ownership of every
process causally started through WSL interoperability. Investigation of Microsoft WSL 2.6.1 source
found service-created interop hosts and root-process channel-loss termination that exempts GUI
applications and is not a tree closure proof. A Windows job covers only its members; it cannot
cover a new Linux process created by the WSL service after a managed command invokes Windows
`wsl.exe`. Beryl's existing Windows job is applied only to Host launches, not the WSL wrapper.
See [exact source and ownership evidence](../memory/topic/wsl-process-supervision/interop-ownership-boundary.md).

Do not accept namespace-init reaping plus wrapper exit as the complete existing cross-OS lifecycle
guarantee. Preserve the approved root privilege boundary, original cleanup custody and replacement
fence, but pause before implementation until either a cross-OS ownership mechanism is qualified or
the Operator explicitly selects an envelope excluding service-created work. Disabling interop or
terminating the shared distribution is not an authorized correction. This is source-backed evidence;
no native interop cleanup test or companion implementation is accepted.
