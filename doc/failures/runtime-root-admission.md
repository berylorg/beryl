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

Implementation is paused. The recommended decision is a narrowly scoped backend-owned WSL probe
that reuses exact process-tree supervision, with application-owned path/environment admission.
Define its fixed inputs, bounded output, cancellation, timeout and retained cleanup custody in the
owning backend transport supplement and backend-runtime system before resuming implementation.
This is a proposed authority correction, not an accepted target contract.

Unaccepted source drafts are retained locally at `.tmp/runtime-root-admission-draft` (seven files,
50,747 bytes; 256 KiB retention limit). They include the incomplete validation/service composition
and tracked-source patch, are excluded from production source and version control, and must be
reconsidered after the ownership decision. Cleanup or replacement of this exact draft scope is due
when the decision is resolved. The root plan records the blocker; phase 733 has no acceptance claim.

Independent source/authority review confirmed the gap and accepted this blocker record with no
blocking documentation findings. The seven touched tracked source files match the accepted HEAD
again, and all six new production source drafts were removed. Heading spacing and scoped whitespace
checks passed; documentation reconciliation returned Current with zero failed or blocked chunks.
