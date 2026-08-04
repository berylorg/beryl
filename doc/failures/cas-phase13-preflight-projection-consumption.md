# Scope

Phase 13 ordinary-turn input preparation and loaded-projection ownership.

# Invalidated Approach

The first ordinary execution API consumed `LoadedCasProjection` at entry and returned a plain error
for preflight failures. Extending that shape with bounded text/image preparation would have let a
preparation or cancellation failure drop the last loaded-projection lease before any CAS byte was
sent.

# Evidence And Failure

Dropping the last `LoadedProjectionLease` deliberately retires its owning projection connection,
because an implicit drop cannot perform an authoritative connection-scoped unsubscribe. Therefore
a missing asset, invalid runtime path, corrupt later marker, or cancellation during bounded
preparation could destroy otherwise exact native CAS context even though dispatch was proven absent.

The existing ordinary execution entry also carried no `ProjectionCancellationToken`, so a page
broker added under that API could not satisfy the system contract that cancellation is checked at
every safe bounded source boundary.

# Required Course Correction

- Perform complete input preparation while borrowing the exact loaded projection.
- Pass one explicit projection cancellation token through preparation and every broker page read.
- On a pre-activation failure, return the exact still-live loaded projection together with the typed
  failure; do not reacquire or reconstruct it as a workaround.
- Consume the projection into a live target only after preparation succeeds and durable activation
  is ready to proceed.
- Once remote dispatch may have occurred, let the existing typed delivery and authority-loss paths
  determine whether any projection can be returned.

# Affected Authority And Proof

The correction affects the app package ordinary-execution API, its package design, Phase 13 plan,
and focused tests. Verification must prove that preparation failure and pre-start cancellation read
no source after cancellation, send no transport byte, preserve a live exact projection, and permit
one later retry with that same capability.

# Unresolved Structural-Failure Boundary

The 2026-07-17 preparation proof invalidated the assumption that every typed pre-activation failure
can both preserve the exact projection and permit a later retry with that same capability. Removing
one committed image sidecar produced the intended typed `InputAssetSidecar::Missing` failure and
returned the still-live projection before `turn/start`, but missing committed bytes correctly fail
the process-wide Beryl-home generation closed.

Restoring the bytes and completing exact same-home recovery publishes a new Beryl-home generation.
`LoadedCasProjection` and `CasProjectionCoordinator` both authenticate the generation on which the
pending Syndic turn and binding were proved, so the retained old-generation projection is rejected
with `ProjectionMismatch` after recovery. Reacquiring ordinary domain handles does not transfer that
projection authority across generations.

This is an architectural contract conflict, not a test-fixture defect. Reclassifying missing or
corrupt committed sidecars as non-structural would violate the accepted fail-closed Beryl-home
architecture. Direct retry and a wrapper-only generation rewrite are both invalid.

On 2026-07-17 the Operator selected the clean cross-generation rebind. One explicit consuming
operation must retain the old lease, reauthenticate the exact recovered home, pending turn, valid
binding, lineage, loaded session, connection, and lease token through bounded stable reads, and only
then produce new-generation projection authority over that unchanged lease. Complete preparation
runs again afterward. No severity downgrade, silent reacquisition, resume, injection, history
reconstruction, preparation-evidence carryover, or test-only bypass is an accepted correction.

# Sealed Content Is Not Canonical-Item-Owned

The first rebind implementation additionally required the submitted content manifest's optional
owner to equal the canonical user-input item. That assumption is invalid: admitted composer
content is authenticated by the canonical item's exact `ContentReference` and the manifest's exact
sealed reference, while manifest ownership is not the canonical-item authority relationship.

All focused streamed-input cases rejected with `InputContentUnavailable` before preparation,
including ordinary text and valid marker-bearing input. The corrected stable read compares the
initial and confirmed manifest records and requires the exact sealed reference. It does not invent
an owner equality absent from the Syndic content model.

# Wrapper-Only Rebind Is Not Recovery Authority

The first Phase 18 rebind implementation remained a public coordinator method over an arbitrary
executable `LoadedCasProjection`. It stabilized the durable pending turn and then rewrote only the
wrapper's home-generation field. That shape could not authenticate the adopted unpublished service
epoch, the exact quarantined lease owner and replacement worker hold, the live registry token, or
the connection-scoped quarantine owner. It also returned capabilities outside any complete-set
ledger, so rejection, retry, disposition, and publication convergence were not enforceable.

Phase 83 removed that direct API instead of extending it. Cross-generation reauthentication now
belongs solely to the adopted-but-unpublished service's consuming candidate ledger. The ledger
keeps every capability owning and non-executable, performs exact pre-read and post-read authority
checks around the shared bounded stable read, and can yield publication authority only after every
candidate is accepted or explicitly disposed and every reauthentication connection owner is
transferred into private converged-service retirement retention. Complete input preparation remains
a later execution operation and no preparation evidence enters the accepted dormant inventory.

# Validate Then Discharge Is Not a Candidate-Set Seal

The first Phase 83 seal separately authenticated accepted registry tokens and connection owners,
then released the connection owners in a later loop. A concurrent connection retirement could mark
the stable core retired after authentication, wait behind the still-installed quarantine owner, and
complete registry-wide token revocation when that later loop released the owner. Seal would still
return a candidate-set-converged authority containing already-revoked dormant tokens. A
zero-candidate connection had the same retirement gap even without token inventory.

Final seal is therefore one all-or-nothing authority transfer. It holds every adopted forwarding
epoch and service-membership barrier, then every connection authority gate in stable order, and
authenticates the complete accepted-token set under one loaded-registry lock. The infallible commit
changes every exact reauthentication barrier into private candidate-set-converged retirement
retention. That retention exposes no connection or execution operation and is released only when
the converged authority is consumed or dropped. Retirement racing the commit either wins before
authentication and blocks seal, or remains fenced after the transfer; it cannot revoke a token in
the middle of a successful seal.

# Retirement-First Is Not A Retryable Candidate Failure

The first atomic-seal correction still treated a stable core that retired before authentication or
before seal as a retryable ledger error. That is invalid after service-epoch adoption commits. The
adopted connection set is exact and all-or-nothing, retirement is irreversible, and connection-
quarantine ownership deliberately keeps the retired core retained. An accepted candidate could
therefore remain in a ledger that could never seal, while an accepted entry had neither a retry nor
an explicit-disposition transition. A zero-candidate retired connection produced the same stranded
service without any candidate to demote.

Post-commit loss of recovered-home, adopted-service, stable-connection, service-membership, or
shared registry-authentication authority now terminalizes the complete adoption. Every accepted,
rejected, or unprocessed candidate that was not already disposed is demoted under one service-wide
reason, and one non-retryable whole-attempt owner retains the candidates, connection owners,
replacement holds, adopted service, and old/new attachments. Its explicit disposition revokes or
confirms every candidate token locally before releasing connection owners and consuming inert
service cleanup; its implicit drop stays bounded and nonblocking. No retired connection is pruned,
including a zero-candidate member.

The successful seal fence does not by itself authorize later service publication. Final recovery
publication must hold every exact stable-connection retirement gate continuously across validation,
process-service installation, and startup-gate opening. Publication-first installs and then
releases converged retention; retirement-first returns the same terminal whole-attempt authority.
A check-then-publish sequence would recreate the invalid race.

# Dormant Provenance Is Not An Executable Recovered Wrapper

Phase 83 also does not construct an executable recovered-generation projection wrapper. Exact
reauthentication transfers the candidate into dormant accepted provenance retaining its recovered
home/service identity, stable loaded identity, durable witness, lease token, and replacement hold.
Executable projection reconstruction and complete input preparation occur only after Phase 84 has
successfully published the recovered service. Creating the wrapper earlier would expose executable
shape before the all-or-nothing adopted set had won its final publication race.

# Bounded Drop Is Not Explicit Unpublished-Service Disposition

The first terminal whole-attempt disposition settled candidate tokens, replacement holds,
connection owners, inert attachments, and stable drivers, then relied on the dormant replacement
service's bounded destructor for the remaining service topology. That destructor intentionally
requests cancellation without joining. The scheduler, compaction coordinator, persistent-failure
coordinator, and scheduled provider could therefore outlive a supposedly completed explicit
disposition, and provider shutdown was not observable.

Terminal disposition now has a dedicated unpublished-inert service path. After startup cancellation
and connection cleanup, it closes the local command gate, requests every service worker to stop,
clears only the unpublished service registry, joins compaction and scheduler workers, shuts down the
provider, and joins the persistent-failure coordinator. It deliberately drops the service's home
reference without closing the retained same-home recovery authority. Cleanup attempts every join
before returning the first typed shutdown failure; implicit drop remains bounded and nonblocking.

# Failed Reads Do Not End The Authentication Envelope

The first Phase 83 stable-read transaction returned a classified candidate-local read error after
checking only final recovered-home health. It skipped the mandatory post-read stable-connection,
adopted-service-membership, and loaded-registry authentication whenever the bounded read itself
failed. A retirement or shared-authority loss concurrent with that read could therefore be hidden
behind a retryable candidate result even though the fixed adopted service had become permanently
unpublishable.

The post-read authentication envelope now closes on every read outcome. The transaction retains
the read result, performs exact post-read shared authentication and final recovered-home validation,
and only then classifies a surviving candidate-local error. Shared terminal loss consequently takes
precedence without discarding the owning candidate or any other fixed-set authority.

# Joined Does Not Mean Cleanly Joined

The first explicit whole-attempt disposition called every replacement provider-ingester join but
dropped each returned stopped owner without validating its terminal receipt. A panicked or failed
ingester therefore looked identical to clean shutdown, so terminal disposition could report success
after a connection worker had failed.

Every adoption cleanup state now validates the stopped ingester's exact home and service generation
and its clean terminal bit before releasing the joined owner. Prepared, bound, inert, old-join-failed,
and adopted attachments propagate failure into the connection-cleanup aggregate. The aggregate still
attempts every remaining connection and service cleanup, then reports typed connection shutdown
failure only after the complete local disposition sequence has run.
