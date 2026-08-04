# Scope

Phase 83 concurrent durable-revision tests after exact same-home recovery.

# Invalidated Approach

The first revision-race fixture retained the pre-failure `SyndicStorage` handle and used it to
publish a synthetic history-summary revision after the home had recovered into a newer generation.

# Evidence

The product replacement service reacquires all Beryl and Syndic domain handles after recovery.
The fixture write instead failed with `ForeignDomain`, while the expected synthetic commit-panic
message came from the earlier intentional persistent-failure cut and was unrelated to the race.

# Course Correction

Any post-recovery fixture read or mutation must use a domain handle reacquired from the exact
recovered home generation. The corrected test reacquires `SyndicStorage`, performs a successful
concurrent durable mutation between the two bounded reads, and proves reauthentication returns an
owning retryable concurrent-change result without terminalizing the adopted service.
