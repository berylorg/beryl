# Scope

Long-running Cargo and nextest verification launched by agents on Windows.

# Invalidated Approach

A delegated Phase 85 verifier used PowerShell `Start-Process` to keep nextest running beyond one
tool-observation window. It omitted `-WindowStyle Hidden`, detached Cargo from the ordinary hidden
tool process tree, and opened a visible tab in the Operator's existing Windows Terminal session.

# Decisive Evidence

The Operator observed the unexpected terminal. Process inspection identified the task-owned
`cargo nextest run -p beryl-app --lib --features test-faults -j 1` process, while the verifier
confirmed the `Start-Process` launch. Cargo became orphaned after its wrapper exited and retained
output through task-created files under `target/`.

# Why It Failed

An observation timeout is not a reason to detach a verification command into the GUI session.
Detachment loses ordinary tool ownership, surprises the Operator, complicates cancellation and
cleanup, and can leave logs or processes after the agent stops.

# Course Correction

Run long Cargo commands through the ordinary yielded shell cell and resume that exact cell with the
tool wait mechanism. Do not use `Start-Process` for Cargo or nextest. If a future bounded background
helper genuinely requires `Start-Process`, it must use `-WindowStyle Hidden`, retain an exact
process handle, bound its logs, and verify cleanup before handoff.

# Affected Work

Phase 85 verification completed successfully after the visible process was identified and allowed
to finish. No user-owned Windows Terminal process or tab was terminated. Four exact task logs under
`target/` remained because shell policy rejected their deletion; agents must not bypass that guard.
