# Native live capture fixture

The native Code fixture must admit runtime interest and budget an ordinary index-construction
worker while the real capture driver is active. A bare lifecycle session is sufficient for
connection admission, but ordinary execution refuses it with `RuntimeInterestUnavailable`.
Use `admit_runtime_lifecycle_test_candidate` on the published service graph.

The general native fixture uses four worker permits: two connection workers, one active ordinary
turn and one protected steering reserve exhaust that budget. Index construction correctly returns
`Busy` until capacity returns. The combined streaming fixture explicitly uses five permits;
existing native cases retain four, and production admission and steering protection are unchanged.
Diagnostic run `ab1c43d9-77af-4289-b71e-253e646ce36c` reproduced repeated construction `Busy`
while saved Code descriptors were already mounted. Temporary diagnostics were removed.

A wire delta can publish several coherent saved-history revisions as its chunks are captured and
projected. The first successor revision is not necessarily the revision containing all requested
lines. Run `6ad38260-ac18-42e4-b920-ef322399572e` loaded 3,000 lines at revision 15 with the source
anchor resolved and sixteen realized lines, while the fixture incorrectly waited for revision 11.
Wait for the expected logical extent at the current saved-history revision, then qualify the actual
provider range and continuity. Do not weaken stale-delivery checks or pin renderer boundaries to
CAS frame or item identities. Panic-path connection loss in these failed native fixtures is not
evidence about normal completion.

The corrected combined run `8e7c4492-85d0-43d6-84cd-9679e3b902f2` passed all 53 selected cases
in 46.097 seconds, including the real-driver native case (14.552 seconds), durable-helper native
case, three native revision timings and 48 mounted-panel cases. Matching completion, terminal
completeness and whole-home scrub passed. This is fixture qualification, not a change to production
capacity requirements or proof of small/empty opening-fence delivery.

The cumulative Code streaming family also outgrew the shared thirty-second whole-fixture watchdog.
Two combined runs reached only `native startup test completion deadline`, then aborted across the
native non-unwinding boundary: saved-code at 31.562 seconds and fragmented tilde at 31.631 seconds.
An exact unchanged saved-code rerun passed in 22.854 seconds; with a case-specific sixty-second
outer bound it also passed in 25.270 seconds during the next combined run. No streaming assertion
failed. Several individually bounded waits, synchronous store/scrub work and teardown share the
outer fixture lifetime, so the correction scopes sixty seconds to the cumulative Code streaming
family. Other mounted families retain thirty seconds and every per-step wait/assertion is unchanged.
Qualify the whole corrected family; one variant's successful rerun does not close combined acceptance.
Record aborted homes from positive launch/log evidence because unwind-based fixture cleanup cannot
run after that native abort.
