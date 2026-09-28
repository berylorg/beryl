# Prepublication Capacity

## Initial Admission Is Not A Preparation Peak Bound

On 2026-09-28, independent review of the initial host-capacity API found that clamping session
availability does not bound all preparation growth. Existing geometry admission builds scanner
budgets from configured geometry limits, and candidate construction allocates its preparation
before checking session availability. Passing initial-effect and generous-budget completion tests
therefore cannot prove capacity reserved for a preserved predecessor remains protected.

The bounded API is retained only as initial ownership admission and an upper bound for existing
availability checks. The root plan separately requires budget propagation before geometry and
candidate growth, tight progressing tests, and independent resource review before using the API
for combined preserved-resident reservation. Do not infer whole-preparation protection from a
constructor ceiling or a later refusal.

Follow-up inspection located additional widget-owned growth before observation in scanner text,
fragment/presentation collections, index/target startup and coherent-surface preparation. GPUI
`d2665f256d20c62ccf1a07c39f549ac05671e11e` exposes finite streaming input/component limits and
checks returned retained charges in `StreamingLayoutSession::finish_admission`; its internal
shaping scratch is not a before-allocation host reservation. Reserve returned custody and local
growth without claiming allocator/RSS protection or changing that dependency-private boundary.
This follows the [bounded-resource contract](../systems/bounded-resource-dataflow/design.md).

Text-buffer admission is separately accepted. Its rollover review demonstrates why a reservation
must be checked against current ownership immediately before growth: shaping the prior segment
can retain geometry between an earlier check and the next text allocation. Charge the detached
grapheme as well as destination storage at that later check. This local correction does not prove
the remaining preparation paths or combined predecessor reservation.

The same custody issue was found for deferred inline objects: in the widget's
`src/range_geometry/exact/scan.rs`, `process_object_page` takes `deferred_object` into a local
box before `admit_inline_object` allocates its run or invokes layout. Scanner counts then omit
that still-live box and fact payload. The inline style-run admission guard alone does not close
this gap. The correction carries that box and fact charge in the admission budget throughout
the nested call, restoring the enclosing budget before propagating success or failure.
Remaining scanner capacity can then include this custody when reserving returned GPUI storage.

Returned-layout reservation now clamps each call's retained byte/item limits to remaining geometry
capacity, including the fragment enum and both pre- and post-replacement continuation accounting.
An attempted conversion of GPUI `Total` errors to geometry capacity errors was rejected in review:
GPUI does not identify which total failed, so reducing the item ceiling could mislabel an unchanged
configured byte-limit failure. Preserve GPUI errors unchanged. Such refusal remains a deterministic
terminal layout-capacity failure; do not infer host-capacity attribution or repeat shaping to
diagnose it. Exact returned capacity can still be followed by a separate checkpoint-growth refusal.

Output-collection growth tests exposed another transition distinction: returned GPUI payload
charges alone do not include the additional fragment enum records retained alongside scanner
backing during transfer. Admit those records and the replacement backing while the old backing
is still charged, before reserving the vector. Exact-cap refusal evidence must inspect released
storage, since a later capacity error can otherwise hide that the allocation already happened.
Use separate setup and scan evidence: index checkpoint peaks and deferred-object continuation
peaks can dominate a test intended to exercise output growth.

## Resident Payloads Retain Their Original Request Identity

A tight-capacity session fixture with repeated emoji and an eight-byte segment limit reached
resident context reuse, then failed with `MalformedResponse` after capacity was restored.
The session selected an authenticated resident page but sent it through ordinary direct response
admission, which requires the newly issued request key. Resident pages retain their original key.
Capacity refusal alone had hidden the mismatch; a fixture that never reused resident pages also
failed to detect it when the new guard was disabled.

Explicit resident admission now reuses the prepared path's binding, revision, presentation and
demand proof without changing external exact-key admission or copying payloads. Cleanup names
the current pending request, not the old resident key. Session integration selects that path
only after residency selection; the context-reuse fixture now resumes through candidate completion.

## Direct Response Ceilings Do Not Provide Retryable Session Admission

The prepublication session previously called destructive direct geometry response admission from
`src/range_widget/prepublication/session/progression/geometry.rs` in the widget fork. Merely passing
reduced host availability into those calls cannot provide retryable response admission:
`ExactGeometryOwner::admit_page_inner` and its object counterpart take the active job, and
`terminal_failure` returns cleanup while dropping that job on capacity refusal. The package's
prepublication contract instead requires a retryable exact-response denial to retain its bounded
custody without advancing. The accepted direct API contract remains valid for terminal callers.

Use the existing prepared-response and explicit commit machinery for the session integration,
with the enclosing host budget applied before preparation growth. Completed-index preparation had
mapped failed successor preparation from a plain error using the earlier outer peak. A shared mutable
preparation budget now preserves attempted peaks, including cleanup storage and saturated overflow
evidence, and merges them before constructing the outer failure. Exact/insufficient capacity and
overflow tests plus independent review accepted that correction; 207 regression tests passed
(nextest run `146b6b87-56d2-4df7-bce1-dc0f60775f8d`). Enclosing failure propagation was reviewed
in source; the focused tests directly exercise production budget and release modules.
The session now uses immutable preparation and explicit commit for delivered and resident responses.
It reuses prepared successor requests through residency and cleanup admission, without allocating
or charging another pending record. Completed-index preparation supplies the target job once.
Blocked successor admission preserves ownership, and cancellation/drop releases exact custody.
Independent review accepted this integration; 208 tests passed (nextest run
`e99b0ccd-ebf7-4ad7-9419-79f1c1f9a2b9`). A subsequent stronger resident test admits current items
while denying the extra prepared-demand record and passed (`d5ea0988-eb92-40d0-b1cf-bc34ea2711f1`).

Response residency admission and geometry preparation now occupy separate bounded work steps.
The admitted state retains exact external waiting identity and resident text/object page identifiers;
capacity below current ownership leaves that state unchanged and retry does not re-admit the payload.
Cancellation, drop and exact-key collision release the original cleanup record exactly once; obsolete
redelivery after commit is rejected. Resident reuse enters the same admitted state. Independent review
accepted this boundary, and all 209 regression tests passed (nextest run
`36cae4c8-e56a-4aa4-b38f-b0965b00eabc`, 19.177 seconds, zero skipped).
The resident refusal fixture now denies current items by one because an admitted preparation step
does not allocate another demand record; the successor fixture accounts for both work steps.

Host scan ceilings remain outstanding. The current-ownership gate does not bound scan allocations:
propagate host availability through immutable preparation, preserve typed retryable attribution, and
account shared presentation ownership once as preparation progresses. Preserve configured terminal
failures and the rule against inferring host attribution from ambiguous GPUI capacity errors.

## Immutable Preparation Ceiling Verification

Internal immutable text/object preparation now accepts enclosing byte/item ceilings, each clamped
to configured geometry limits. The existing budget carries them through scan, publication and
completed-index successor preparation. Ordinary callers supply configured limits. This primitive
does not yet derive a session ceiling or classify retryable host denial.

The integration fixture covers index/target and resident/external paths, zero/exact/one-under
ceilings, unchanged owner counts, reusable pending identity, empty refusal release and successful
commit after refusal. Reconstructed owners also prove oversized enclosing allowances cannot
override configured byte or item limits. New tests were moved from the existing private unit-test
module convention to Cargo integration placement, with a feature-gated probe delegating directly
to production preparation/commit. Independent review accepted propagation and the test-support
delta. Final run `29b4ecf4-2536-46c8-a3a1-34d7339eb2ac` passed all 210 integration tests
(19.487 seconds, zero skipped); focused moved-fixture run `f0e8e45f-be19-4601-ba39-8c91fb7026ae`
also passed.

The expanded run `2d65e52d-54ec-4d71-a468-e88f24d9103e` passed 320 of 325 tests, including all
209 integration tests. Five older unit tests failed and reproduced on unchanged widget `a3f9fc8`
in baseline run `029ff07b-a1a5-422d-8771-0dbb7f7bead9`:

- `committed_settlement_accepts_exact_fit_and_one_under_is_retryable`: fixed component charges differ.
- `terminal_target_replacement_accepts_fixed_exact_caps_and_rejects_one_under`: resident charge is
  807 bytes, while the fixture expects 783.
- `history_custody_capacity_exhaustion_releases_and_reuses_exact_slots`: restoration result differs
  from the expected `NotQuiescent` refusal.
- `active_interaction_and_scroll_anchor_are_runtime_realization_targets`: observed `ScrollAnchor`
  differs from the expected `ActiveInteraction`.
- `exact_priority_after_end_object_retains_proof_for_successive_edit`: observed count is four,
  while the fixture expects one.

Do not treat those baseline failures as a green full unit suite or infer their remedy from assertion
text. Reconcile each against current authority before changing expectations or production behavior.
