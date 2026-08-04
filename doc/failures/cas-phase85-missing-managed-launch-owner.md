# Invalidated Approach

Phase 85 initially described selecting the installed CAS 0.146.0 executable for Beryl-owned
launches but did not name restoration of the production managed-launch owner as an implementation
task. When source inspection found that only process-supervision and test-connector pieces remained,
the missing owner was mistakenly treated as an external prerequisite.

# Why It Failed

The authoritative backend-runtime and package designs already assign exact-path launch, auth,
process lifetime, and connector creation to Beryl. Their absence from current source is therefore
unfinished implementation inside the atomic cut, not an operator prerequisite. A version-only or
parser-only cut would also be architecturally incomplete because no production boundary would
select the exact executable or authenticate that compatibility evidence came from the process Beryl
launched.

# Course Correction

Phase 85 now explicitly restores the production managed-launch owner. It consumes validated Host or
WSL runtime identity, launches the exact executable with strict managed configuration, owns auth and
the complete process boundary, and creates the only production connector. Compatibility admission
must bind its effective native-spawn proof to that launch provenance and fail closed when either is
missing. Missing implementation steps that are already implied by live authority should be added to
the active plan and implemented rather than promoted to external blockers.
