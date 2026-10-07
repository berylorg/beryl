# Raw Namespace Clone And Process Identity

The native supervisor originally used libc process and signal wrappers immediately after raw
`clone3` namespace creation. Native nextest run `59d2681f-8711-4c02-ac5e-3d7096ac3621` refused
startup with `ESRCH`, before positive execution confirmation. The working assumption that those
wrappers would use the child's new identity was invalid in this path. Inherited musl thread-local
process identity is the implementation diagnosis; the observed failure alone does not establish
the internals of every libc version.

The child now obtains its immediate kernel process/thread IDs and stops itself through direct
`SYS_tgkill`; credential establishment likewise uses direct credential syscalls. Native run
`83b5908b-5849-420d-97ef-8fa0e4cd4c8e` then passed all five namespace lifecycle cases, including
execution refusal, detached descendants and lost control. This correction preserves positive
ptrace execution confirmation and original pidfd ownership; it does not substitute EOF or numeric
process identifiers for disposal proof.

The remaining acceptance boundary is the complete native suite and production consumer review in
[the active plan](../plan.md), under [companion authority](../../crates/beryl-wsl-supervisor/doc/design.md).
