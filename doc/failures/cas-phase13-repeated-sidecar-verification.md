# Scope

Phase 13 marker-bearing ordinary-input asset preparation.

# Invalidated Approach

The first prepared-input implementation verified the referenced sidecar bytes before deciding
whether an image label was a first occurrence or a repeated textual reference.

# Evidence And Failure

Sidecar verification reads and hashes the complete content-addressed file under a bounded buffer.
A valid draft may repeat one label up to the 1,024-marker input ceiling, while one image may be as
large as 512 MiB. Verifying before repeat classification would therefore perform the same complete
file read up to 1,024 times even though CAS receives the image path only once.

The operation would remain memory-bounded but would violate the required linear work boundary in
logical input and could turn an ordinary repeated reference into hundreds of GiB of unnecessary
disk reads.

# Required Course Correction

- Prove every marker's exact submitted-item owner reference and asset identity.
- Classify the label as first or repeated before sidecar verification.
- Verify and retain one sidecar guard only for each first label occurrence whose local-image path is
  emitted.
- Repeated occurrences retain their distinct owner evidence and generated `[Image X]` text but do
  not reopen, rehash, project, or resend the sidecar.

# Affected Authority And Proof

The correction affects app prepared-input ownership and the CAS-live system contract. Focused tests
must prove that many repeated markers produce one local-image item and one sidecar verification per
independent descriptor traversal while all owner references remain exact. The final replayable
source performs one digest-building traversal during preparation and then one traversal each for
the request, started echo, and completed echo, so one successful turn verifies each first image four
times in total. Repeated markers add no sidecar verification in any traversal, and at most one
verification handle is resident at once.
