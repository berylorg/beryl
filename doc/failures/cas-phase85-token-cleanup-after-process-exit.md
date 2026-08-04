# Scope

Managed Codex App Server authentication-token cleanup during Phase 85 shutdown.

# Invalidated Approach

`ManagedBackendServer::Drop` preserved token material after any shutdown error. Shutdown stopped the
managed process before joining its stderr reader, so a later reader-join failure could preserve the
token even though no live process boundary remained.

# Decisive Evidence

The independent Phase 85 completion review traced the ordering through `ManagedBackendServer` and
found that one undifferentiated shutdown error covered both process-stop failure and post-stop
stderr cleanup failure. That contradicted the system contract requiring per-run token material to
be deleted after process shutdown.

# Why It Failed

Token preservation was keyed to the overall shutdown result instead of the security-relevant
process boundary. Diagnostic-reader cleanup does not retain authority to use the token once the
supervised process is confirmed dead.

# Course Correction

Track whether the supervised process boundary has been released. Preserve token material only when
process termination fails and a process may remain live. After termination succeeds, attempt token
deletion before joining stderr; a later join error remains visible but cannot authorize token
preservation. The regression injects that exact post-termination join failure, requires the typed
error, and proves the token file was already removed.

# Affected Work

Phase 85 managed Host/WSL launch ownership and cleanup. The production launch, process supervision,
and authentication architecture remain unchanged.
