# Scope

Phase 68 of the CAS-live Syndic stop-operation integration.

# Invalidated Approach

Mount the app stop coordinator directly from the accepted mutation and reconciliation APIs while
the app independently reads or retains the gate, selected route, turn, binding, execution snapshot,
published CAS turn, and optional stop record needed to construct those requests.

# Evidence

`syndic-storage` exposes `AdmitStopOperation`, `JoinStopCause`, `ClaimStopDispatch`,
`SafelyReopenStopOperation`, `AbandonStopOperation`, and their exact transition-status reads.
However, its only stable multi-record stop observation is private under `src/read/stop/`; no keyed
public read returns the complete coherent current stop-admission source for one Syndic thread.

The app router retains activation-time facts, but path-neutral accepted-input admission may advance
the gate and selected-route revisions while preserving the immutable active target. Combining
separate point reads in the app would duplicate storage consistency policy and could mix
cross-revision authority before a mutation rejects it.

# Course Correction

The root plan now establishes a bounded keyed Syndic stop-admission read as its own phase before the
app mount. Storage owns stabilization, exact target authentication, live-stop agreement, and closed
ineligible classification. The app will consume that composite authority without reconstructing
the storage join.

# Remaining Risk

Completion review must verify that the new read reuses canonical validators, remains fixed-work,
and does not expose a clone-heavy aggregate or a second durable authority.

# Invalidated Reduced-Facts Implementation

The first implementation reused the startup delivery-recovery fact set as its final exact-target
and live-stop authenticator. That set did not include the thread and binding head, CAS thread
reservation, CAS thread membership, CAS turn reverse index, or accepted ready and next source
records required by the canonical stop mutations and reconciliation reads.

A stable missing reverse record could therefore still produce an admissible target or live stop.
The implementation was rejected in independent review before phase completion.

# Architectural Correction

The keyed read now uses delivery-recovery facts only to discover the current candidate. Each
steerable or stopping pass additionally reads the complete canonical stop observation and validates
it through the shared target, reverse-authority, route-source, and live-stop authenticator. Both
complete raw passes must match before semantic classification runs or any compact public result is
returned. This ordering prevents a legitimate atomic transition inside the first pass from being
misreported as stable corruption.
