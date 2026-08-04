# Invalidated Approach

Naming a Windows Rust integration-test executable with `dispatch` is unsafe in this repository.
The generated executable name contains the substring `patch`, which can trigger Windows installer
detection and make ordinary nextest discovery fail with elevation error 740.

# Correction

Use a neutral test-target name such as `request_flow`. Source modules and conceptual design may
still use dispatch terminology; only the generated Windows executable name needs to avoid the
installer heuristic.

# Evidence

On 2026-07-20, the Phase 31 test target compiled successfully as `phase31_bounded_dispatch`, but
nextest could not execute its `--list` command because Windows reported that elevation was
required. Renaming the Cargo test target to `phase31_request_flow` removed the invalid executable
name without changing production behavior.
