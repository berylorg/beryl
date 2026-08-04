# Scope

Hard-stop escalation after an ambiguous primary interruption.

# Invalidated Approach

Phase 64 initially allowed the hard runner to issue frozen target requests
after the primary `turn/interrupt` reached completion unknown.

# Why It Failed

Completion unknown requires retirement of the exact foreground session before
another poll or request. Hard-target operations are required to use that same
authenticated session and are forbidden from reconnecting, resuming, or
opening a detached request-only client. The proposed sequence therefore
destroyed the only dispatch capability before trying to consume it.

A source-pinned rejection has the same practical result for a different
reason: it does not prove that the selected parent target remains current, so
the frozen handles are no longer authorized.

# Course Correction

Hard escalation may begin only after matching primary response acceptance or a
local pre-byte nondispatch outcome while the same foreground session and parent
target remain exact. Provider rejection or primary completion unknown marks
every unattempted frozen target unavailable without issuing another request.
Partial failure remains per-target only while the exact session survives.

# Affected Authority

Phase 64 reconciles the CAS-live system, status-line feature, and app package.
Backend session retirement remains fail-closed, and later implementation may
not add a detached escalation connector.
