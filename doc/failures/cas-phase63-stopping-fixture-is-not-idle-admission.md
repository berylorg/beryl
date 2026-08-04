# CAS Phase 63 Stopping Fixture Is Not Idle Admission

## Invalidated Approach

The ordinary runtime next-input fixture reused the new stopping-input helper and changed the gate
back to idle after admission.

## Why It Failed

The ordinary fixture's parent is already complete. A `Stopping(parent)` gate is valid only while
that exact parent still blocks the owning thread. Domain validation therefore rejected the
intermediate fixture before it could be changed back to idle. A later overwrite cannot make an
invalid committed state acceptable.

## Course Correction

The ordinary scheduler fixture now establishes a genuine active predecessor, queues the accepted
input under that exact `Stopping(parent)` authority, and terminalizes the predecessor before
waking next-turn scheduling. The restart-cut fixture retains the same valid stopping state across
reopen. Both paths validate immediately after each admissible durable transition.
