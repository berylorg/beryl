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
