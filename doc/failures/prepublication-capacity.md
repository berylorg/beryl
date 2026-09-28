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

The prepublication session currently calls destructive direct geometry response admission from
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
Account shared presentation ownership once as preparation progresses. Session integration and its
focused retry/cleanup tests remain outstanding. Preserve configured terminal failures and the existing
rule against inferring host attribution from ambiguous GPUI capacity errors.
