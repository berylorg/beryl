# Windows Symlink Tests Require Privilege

## Invalidated Approach

Treat the complete `beryl-home-store` nextest suite as an unconditional non-elevated Windows gate.

## Evidence

The all-feature supporting-crate run passed 593 of 595 tests. The two remaining tests failed before
entering Beryl behavior when `symlink_dir` and `symlink_file` returned Windows error 1314: the client
did not hold the required symlink privilege.

The affected cases are `directory_symlink_reaches_one_opened_object_and_lock` in `home_aliases` and
`elevated_exact_content_final_symlink_and_final_directory_are_structurally_rejected` in
`sidecar_phase13`.

## Why It Failed

Creating Windows symbolic links can require an elevated token or an enabled developer capability.
An ordinary test process cannot manufacture that authority, and the failure occurs before the
storage implementation is exercised.

## Required Course Correction

Keep both cases as privileged filesystem tests. For an ordinary non-elevated gate, run the complete
suite except those exact cases and report the privilege exception separately. Do not weaken their
assertions or emulate symlinks with another filesystem object.

## Affected Work

Windows verification of `beryl-home-store` owns this gate qualification. It is not a Phase 38
submitted-input residency failure.
