# Reason For Investigation

Phase 12 diagnostic-acceptance verification intermittently failed after production cleanup had observed `Child::try_wait()` return a terminal status, released the kill-on-close Job, and joined its owned writer. The test treated successful `OpenProcess(pid)` as proof that the exact process was still running.

# Outcome

`OpenProcess` success is not a running-state predicate. A terminated Windows process object may remain openable while a handle or kernel reference retains it, and a PID can later identify a different process after the original object is freed. PID-only absence polling therefore cannot prove exact cleanup and can fail after a valid reap.

Tests that own the exact child should retain or duplicate its process handle before cleanup, then use the signaled handle state or `GetExitCodeProcess != STILL_ACTIVE` after cleanup. External PID probes additionally need immutable creation identity to exclude reuse. Writer and reader completion should be asserted through their owned join state rather than inferred from PID disappearance.

The affected focused nextest case passed 20 consecutive repetitions after the one grouped-load failure, consistent with an invalid timing/identity assertion rather than a production cleanup-order defect.

# Sources

- Microsoft, "Process Handles and Identifiers," https://learn.microsoft.com/en-us/windows/win32/procthread/process-handles-and-identifiers, accessed August 5, 2026. Defines process-object lifetime and PID uniqueness limits.
- Microsoft, "PROCESS_INFORMATION structure," https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/ns-processthreadsapi-process_information, accessed August 5, 2026. Describes retained process and thread handles.
- Microsoft, "GetExitCodeProcess function," https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess, accessed August 5, 2026. Provides the active-versus-terminated state check.
- Microsoft, "ExitProcess function," https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-exitprocess, accessed August 5, 2026. Describes termination behavior.
- Microsoft, "Kernel Objects," https://learn.microsoft.com/en-us/windows/win32/sysinfo/kernel-objects, accessed August 5, 2026. Defines handle/reference lifetime.
- Microsoft, "Job Objects," https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects, accessed August 5, 2026. Defines Job ownership and kill-on-close behavior.
- Rust standard library, `std::process::Child`, https://doc.rust-lang.org/std/process/struct.Child.html, accessed August 5, 2026. Defines `try_wait` and child-handle ownership.
- Local production and test sources: `crates/beryl-app/src/diagnostic_child_supervisor.rs`, `crates/beryl-app/src/diagnostic_child_supervisor/transport.rs`, and `crates/beryl-app/tests/diagnostic_child_supervisor.rs`.
