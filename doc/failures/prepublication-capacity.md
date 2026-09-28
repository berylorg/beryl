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

The same custody issue remains for deferred inline objects: in the widget's
`src/range_geometry/exact/scan.rs`, `process_object_page` takes `deferred_object` into a local
box before `admit_inline_object` allocates its run or invokes layout. Scanner counts then omit
that still-live box and fact payload. The inline style-run admission guard alone does not close
this gap. Preserve or explicitly charge deferred custody through admission before relying on
remaining scanner capacity for the returned GPUI reservation; this remains phase 747 work.

Output-collection growth tests exposed another transition distinction: returned GPUI payload
charges alone do not include the additional fragment enum records retained alongside scanner
backing during transfer. Admit those records and the replacement backing while the old backing
is still charged, before reserving the vector. Exact-cap refusal evidence must inspect released
storage, since a later capacity error can otherwise hide that the allocation already happened.
Use separate setup and scan evidence: index checkpoint peaks and deferred-object continuation
peaks can dominate a test intended to exercise output growth.
