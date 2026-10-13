# Verified Sidecar Instance Qualification

Opaque verified-byte authority must bind its verifying store instance as well as its healthy
generation. A generation number is scoped to one HomeStore lifecycle, not globally unique.

The 2026-10-13 historical-import witness reused `VerifiedSidecar` by comparing its address and
numeric generation with the supplied healthy store. Focused runtime tests showed that both a
foreign home with the same image and a fresh open of the original home accepted the old token.
`HealthGate::opening` initializes each lifecycle at generation one, and the original verified
token retained only address, path and generation. Thus equality did not prove the required scope.

The corrected [physical repository contract](../../crates/beryl-home-store/doc/design-physical-repositories.md#sidecar-publication)
retains the existing private store-instance identity with the verification generation. HomeStore
owns bounded current-scope qualification; consumers also check their exact required address.
Qualification preserves typed health failures and requires no rehash, path-based identity,
persisted record or new global registry. The historical witness uses that storage-owned check.

Acceptance must preserve successful same-instance reuse while rejecting foreign homes, fresh
close/reopen and retired recovery generations even when address or numeric generation matches.
Previously verified bytes do not by themselves establish current operation authority.

Follow-up review found the same distinction at the prepared import-command boundary. Consuming an
original image proof produced a prepared advance that retained only numeric generation and
canonical records, then a factory wrapped it with whichever storage handle was supplied later.
Identical durable state in a fresh open could therefore receive a marker qualified in the retired
instance. Prepared import capabilities must retain their originating domain scope through command
contribution; current record equality cannot replace that scope. Acceptance must also transport a
genuinely consumed prepared marker step to foreign or freshly reopened storage and reject it.
