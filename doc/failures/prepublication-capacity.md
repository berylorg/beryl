# Prepublication Capacity

## Shared Inline Output During Object Scanning

On 2026-09-28, enclosing object scans began carrying shared inline output credit through actual
GPUI reservation and scanner custody. Startup admits only existing credit. The known inline input
supplies prospective display allowance; returned pointer/length identity then proves the credited
payload. One stack counter carries output credit through destination reservation, retained fragments,
metadata and subsequent layout. Returned/retained handles do not duplicate credit because their
payload is charged once. Discarded output is dropped before its credit is removed. Detached fact
credit remains separate, and scanner credit is cleared before response finalization/publication.
Configured and geometry-relative counts stay raw; no persistent owner state or registry was added.

Actual scanner probes cover two page objects and deferred layout for index/target paths, empty and
multibyte displays, exact limits, one-under bytes/items, low/high current baselines, overflow,
unchanged original owners, empty failure releases and retry. The page probe checks retained credit
against live fragments and zero credit after discarded index output. Existing overlap fixtures
qualify independent backing, repeated page references and returned/retained aliases; existing
observer/startup tests qualify configured precedence and startup isolation compositionally.

Focused runs `24622bdd-4968-478b-93e3-9d957c38e7fc` and
`29cba84c-0eec-4acc-9b5d-811fc6b850b6` exposed outdated test assumptions: detached layout now
credits both the fact copy and returned display, and a one-under limit can reach GPUI's ambiguous
`Total` refusal before a later explicit observation. Tests preserve that unattributed failure rather
than fabricating host attribution. Final LLVM, one-job, nonincremental, debug-zero run
`69bdf163-c018-42cc-9685-7a8b81a4a62b` passed all 226 integration tests across `range_widget`,
`exact_geometry` and `prepublication`, no skips, in 23.245 seconds.
Independent resource review found no blocking issues in this boundary. No source or tests changed
after the successful run.

Active object-response publication now carries candidate-only output credit plus deferred display
whose allocation matches the borrowed object page or original continuation. The scanner counter is
cleared before explicit publication observations; its retained output amount and the proven deferred
amount are checked and applied once to successor-record and destination admission. The original
owner and response pages remain borrowed throughout. Configured counts, records, fallback text and
items stay raw; terminal/source-finalization and text-response paths still supply zero credit.

Whole-response index/target fixtures cover initial deferred tails and subsequent object responses,
empty/multibyte display, low/high current baselines, unchanged raw peaks, reduced enclosing byte
peaks, exact fits, one-under bytes/items with explicit enclosing refusal, repeated preparation,
unchanged originals and empty failure releases. LLVM one-job, nonincremental, debug-zero run
`79511000-e798-458b-bb68-877ec2926ef6` passed all 226 integration tests across the same three targets,
no skips, in 21.283 seconds. Existing observer tests and unchanged common admission establish
configured precedence and overflow handling compositionally. Independent resource review found no
blocking issues; no source or tests changed after the full successful run.
The new cases directly cover `NeedObjects`; `NeedContext` is qualified by source review. Exact-fit
tests use observed peaks rather than an independent publication-byte oracle.

This proves scanning and active object-response publication, not fully credited response peaks.
Complete object pages now also carry retained scanner output credit through nonterminal forward-text
publication. Previously that branch supplied zero to successor and destination admission, inflating
enclosing byte peaks despite shared backing. The counter remains cleared after scanning; a stack
argument supplies the proven candidate-only amount only to the nonterminal branch. No original
output is credited again, and source-finalization and terminal/nested branches remain unchanged.

The extended `text_successor_retains_shared_output_and_consumes_deferred_custody` fixture caught
the missing credit before the fix (`10834565-f530-4e09-bb93-8656bd1f61f0`). It covers index/target,
delivered/resident, empty/multibyte display, low/high baselines, raw-peak parity, exact observed
enclosing fit, byte/item one-under, unchanged original custody and retry. One-under may reach
GPUI's unattributed `Total` refusal; the fixture preserves that distinction. Following text-response
checks still prove zero deferred credit. Independent review found no blocking issues; ownership
is source-reviewed and exact-fit limits use observed peaks, not an independent publication oracle.
Final LLVM, one-job, nonincremental, debug-zero run `50e45456-c273-4096-bb01-c4411c007caf`
passed all 227 integration tests across the same three targets, no skips, in 21.265 seconds.
No source or tests changed after that run. Root phase 808 and widget phase 63 accept this boundary.

Source finalization now carries the retained scanner display credit through final layout calls and
terminal checkpoint admission. Existing inline fragments remain live, and new non-inline output
receives no additional credit. A scoped helper restores the prior stack counter on success and
error before terminal publication. Original ownership and borrowed input lifetimes remain unchanged.

The isolated production finalization fixture covers index/target, empty/multibyte display, low/high
current charges, unchanged raw peaks, exact-fit and byte/item one-under limits, unchanged original
custody, empty releases and retry. Expected enclosing peaks are raw finalization peaks mapped by
the known baseline and retained display length; this is not an independent raw allocation oracle.
The probe checks restored credit on every returned result. Red run
`37bd944d-270a-495f-b636-01d4e698c202` rejected the intended fit at finalization before the fix;
focused green run `e23e8935-2052-410a-a127-cc9f26c54d8e` passed afterward. Full LLVM, one-job,
nonincremental, debug-zero run `86b4a7a1-a4fe-4c0d-9a78-dc9fb78a5f65` passed all 228 integration
tests across the same three targets, no skips, in 22.472 seconds. Stable formatting and scoped
diff checks passed. No source or tests changed after that run. Ambiguous GPUI `Total` refusal stays
unattributed; configured precedence and overflow remain covered by existing observer tests.
Independent semantic/resource review found no blockers, including scoped restoration and the
production call path. Root phase 809 and widget phase 64 accept this finalization boundary.

Terminal target publication now carries candidate-only scanner display credit through array
staging/conversion, publication allocation and cleanup-record admission. Fragment handles move
into the target before scanner vectors clear; their output charge remains in the raw observation.
Original payload already in the baseline receives no additional display credit, and existing
payload-transfer deductions remain unchanged. The terminal call consumes its stack budget.

The new whole-response fixture covers delivered/resident inputs, empty/multibyte display, initial
output and prior committed output plus a deferred tail, zero/high current charges, raw peak parity,
exact fit, byte/item one-under, unchanged owner, empty failure releases and retry. Expected credit
uses known display lengths against observed raw peaks, not an independent raw allocation oracle.
Early target completion is qualified by source review; the fixture exercises source-end completion.
Red run `ce99f545-433d-4c2e-a585-f429d100a4a6` showed the missing nine-byte credit; expanded green
run `27579f9c-03d1-428e-a90e-99aa9cde95cb` passed. Full LLVM, one-job, nonincremental, debug-zero
run `24bdb491-ebca-4208-8803-a54dcbde9aca` passed all 229 integration tests across the same three
targets, no skips, in 22.926 seconds. Formatting and scoped diff checks passed; no source/tests
changed after the full run. Configured precedence and arithmetic remain covered by existing
observer tests; ambiguous GPUI refusal remains unattributed. Independent semantic/resource review
found no blockers, including early completion routing, array backing lifetime and unchanged original
payload deductions. Root phase 810 and widget phase 65 accept this terminal target boundary.

Terminal index/nested publication and live session budget routing remain separate work.

## Detached Deferred Display Custody

On 2026-09-28, enclosing response preparation began carrying the detached deferred fact's existing
display credit through its entire inline admission. The original owner remains borrowed. A scoped
budget helper adds the full detached record/fallback/display charge and four items while recording
only the copied display as shared. All explicit observations and continuation startup retain full
configured charges. GPUI allowance uses that existing credit with zero prospective output credit.
Checked arithmetic precedes mutation; returned success/error restores raw fixed counts and credit
before further scanner work. Direct and geometry-relative admission initialize and retain zero
credit. This adds one stack budget counter, no retained owner state or allocation registry.

The feature probe isolates actual deferred layout after a separately admitted continuation copy.
Index/target fixtures cover empty and multibyte displays, exact enclosing limits, one-under byte/item
refusals, low/high current baselines, overflow without host attribution, source-contract failure,
empty releases, unchanged original owners and retry. Every returned result checks scope restoration.
Whole-response enclosing preparation also verifies shared backing and raw peak parity. The probe
does not establish fully credited response peaks; returned output and publication remain separate.
Configured precedence is compositionally covered by existing observer/response tests and the common
admission path, not a new tight-configured detached fixture.

Initial full run `c807a7d4-a3aa-4472-a82c-4b83868c5a7d` passed 225/226: a new assertion incorrectly
expected an earlier byte refusal to reach the successful operation's later item peak. It now checks
the actual attempted peak exceeds the refused limit and stays within the successful peaks. Focused
run `1c0f1ff3-baac-4ddf-bda7-6592e8cb3cf1` passed. A subsequent source-contract assertion initially
compared an error reference to a value; that fixture compile error was corrected. Final LLVM,
one-job, nonincremental, debug-zero run `09cb601a-35ee-4cb4-bb93-19d9928ed60e` passed all 226 tests
across `range_widget`, `exact_geometry` and `prepublication`, no skips, in 20.982 seconds.
Independent resource review accepted custody, checked mapping, startup/allowance separation and
restoration. No source or tests changed after the final successful run.

## Shared Display Credit At Deferred Tail Creation

On 2026-09-28, enclosing response preparation began crediting the new deferred tail's display at
pre-allocation and immediate retention observations. The borrowed object page remains in the raw
baseline and its display is shared by `clone_for_geometry`. The scanner uses one helper for both
observations. Configured and geometry-relative charges remain raw; fallback copies, all records and
items remain fully charged. Direct admission supplies no credit. No persistent state or allocation
registry was added. Detached custody, GPUI output and publication remain separate unfinished credits.

The feature probe isolates this production helper after preparing a continuation under configured
limits. Index/target fixtures cover empty and multibyte displays, exact fit and one-under byte/item
ceilings, identity geometry limits, current charge below/above the baseline, overflow without host
attribution, retries, empty failure releases and unchanged original owners. Whole-response enclosing
fixtures also exercise the production selector and verify shared backing and unchanged raw peaks.
The tight probe does not establish a fully credited whole-response peak: later zero-credit
observations can still dominate. Configured precedence remains covered compositionally by the
existing observer/response tests and inspected common admission path, not a new tight-configured
deferred-tail fixture.

Initial nextest run `019374c9-3955-4f06-b7c2-94f53661cbe8` passed 226/226 in 20.545 seconds.
After adding the whole-response enclosing fixtures, final LLVM, one-job, nonincremental, debug-zero
run `7b00e205-fb2e-47d4-b04a-53b1966338b1` passed all 226 integration tests across `range_widget`,
`exact_geometry` and `prepublication`, no skips, in 20.388 seconds. Independent resource review
accepted backing custody, boundary-local credit, full configured accounting and unchanged-owner
failure/retry behavior. No source or tests changed after the final successful run.

## Deferred Display Credit At Continuation Copy

On 2026-09-28, text and object preparation gained one shared continuation-copy boundary that
admits the copy before allocation and observes its immediate coexistence with the original.
Only enclosing mode credits the deferred fact's display length, whose backing `clone_for_geometry`
shares with that original. Configured observations and geometry-relative limits retain the full
raw charge. Fallback text, records and semantic items receive no credit. No extra retained state,
page registry or persistent allocation is introduced. Subsequent observations still use zero
credit; this is not completion of scanner/publication credit or session routing.

The feature probe executes that actual boundary with the original owner's baseline and drops the
copy. Integration coverage exercises index and target jobs, absent deferred state, empty and
multibyte displays, exact fit, one-under byte/item rejection, enclosing current charge below and
above the raw baseline, overflow without fabricated attribution, repeated retries and unchanged
original custody. Existing mapped observer and actual response tests retain configured-priority
coverage. The first run (`20ebbc12-1998-47f3-8e28-030b69537db9`, 225/226) exposed a fixture-only
component limit: the multibyte display exceeded the previous eight-byte layout bound during setup.
Increasing that fixture bound to 16 admitted its intended input. LLVM, one-job, nonincremental,
debug-zero nextest run `8d72f1b5-1aa6-409b-8f20-91adb05cb4db` passed all 226 integration tests,
no skips, in 20.717 seconds. Independent resource review accepted the copy admission, backing proof,
raw configured charging, checked mapping and drop/unchanged-owner behavior. A direct tight configured
copy fixture is not added: configured precedence is qualified compositionally by existing tests
and the inspected unchanged observer. No source or tests changed after the successful run.

## Actual Response Baseline Mapping

On 2026-09-28, immutable text and object response preparation gained a typed internal capacity
input that distinguishes geometry-relative ceilings from enclosing current charge and limit.
The enclosing form derives its raw baseline from checked owner counts plus borrowed response
inputs. It retains configured geometry limits as raw ceilings and carries the existing mapped
observer through scanner and nested publication admissions. Ordinary wrappers retain identity
mapping. No session routing or shared-display credits are enabled by this constructor plumbing.

The existing response integration matrix now exercises index/target and resident/delivered paths
with enclosing baselines below and above the raw baseline, exact fit, one-under bytes/items,
configured refusal, stale validation, arithmetic overflow, retry after refusal and unchanged owner
counts. Empty object-page fixtures explicitly charge their one page record; the first compile
attempt tried to call the private allocated-item accessor from an integration test and was corrected
without changing production visibility. LLVM, one-job, nonincremental, debug-zero nextest run
`f59aaa9f-1de3-4638-b7b1-013fd896ed0c` passed all 226 integration tests across `range_widget`,
`exact_geometry` and `prepublication`, no skips, in 20.369 seconds.
Independent resource review accepted the typed constructor, mapped scanner/nested custody,
validation ordering and refusal evidence. No code or tests changed after that successful run.

The enclosing caller must already account for the geometry owner and borrowed inputs in its
current charge. Per-observation shared-display credits, live session baseline derivation and
typed retry routing remain required before completing session capacity enforcement.

## Presentation Overlap Before Scanner Metadata

On 2026-09-28, active scanner overlap stopped depending on target presentation metadata, which
does not yet exist when inline GPUI output first returns. It now queries the inline fragments'
live presentation pointer and length against bounded object-page allocations. Deferred facts keep
their separate overlap term. The same query qualifies returned fragments before scanner insertion;
ordinary text and unrelated equal content receive no credit, empty displays contribute zero bytes,
and repeated page references do not multiply credit. No session admission credits are enabled yet.

The query counts charged fragment occurrences, not distinct allocations. In particular, callers
must not concatenate returned and retained handles when the scanner already charges their shared
payload once; `scan/output.rs` separately charges only duplicate fragment records at that peak.
Future credit wiring must retain that distinction and account detached facts explicitly.

Actual GPUI qualification covers empty/nonempty shared and independent backing, repeated page
references, duplicated charged occurrences, and ordinary text using the same display backing.
Existing deferred index/target preparation and commit coverage remains green. The first test build
required converting GPUI's immutable fragment slice to a vector before extending the fixture;
run `b8f153ab-fc92-4007-8c2c-7e36c016fba1` then passed the existing 225 tests but rejected the new
nonempty fixture's undersized baseline. Using the established 64/40 height/baseline resolved it.
LLVM, one-job, nonincremental, debug-zero nextest run `9a0de84a-b271-4775-9908-f657e1d18bbb`
passed all 226 integration tests across `range_widget`, `exact_geometry` and `prepublication`,
no skips, in 21.436 seconds. Independent resource review accepted this ownership-query boundary;
session baseline mapping and observation-specific credit application remain separate work.

## Shared Output Capacity Allowance

On 2026-09-28, the mapped allowance calculation gained separate existing and prospective shared
credits. Existing credit must fit raw preparation growth and is subtracted only after checked
baseline addition. Prospective credit extends only enclosing output headroom; configured capacity
and representability of the pre-credit sum still bound the result. The extension is bounded before
addition, so maximum-sized prospective credit cannot wrap. This is a pure calculation and does not
replace startup admission or create attempted-peak/refusal evidence. Ordinary callers continue to
supply zero credits; scanner ownership plumbing and live session mapping remain pending.

Integration qualification exhaustively compares bounded allowances with actual observation
admission for every candidate output, using only the prospective credit represented in that output.
Additional cases cover independent byte/item ceilings, configured precedence, invalid existing
credit, baseline underflow, checked addition overflow and near-maximum allowance representability.
LLVM, one-job, nonincremental, debug-zero nextest run `e568d861-b790-4585-8760-dce5338cc665`
passed all 225 tests across `range_widget`, `exact_geometry` and `prepublication`, no skips,
in 20.470 seconds. Independent resource review accepted the calculation and its bounded scope.

## Explicit GPUI Continuation Startup Admission

On 2026-09-28, the widget began admitting GPUI continuation startup explicitly before resuming
layout. The existing occupied reservation includes transient style runs, the returned fragment
record and the larger prior/successor continuation item allowance. The new boundary adds the
actual GPUI continuation struct and semantic items for its prior position, using checked sums and
the ordinary zero-credit admission path. Configured/enclosing failures now carry explicit
attribution and attempted peaks before GPUI is called. Errors actually returned by GPUI remain
unattributed; component limits and output allowances are unchanged. No GPUI API change is needed
to enforce this startup boundary. Live session mapping and prospective output credits remain pending.

The new integration probe compares startup charges with an actual GPUI session at positions with
zero, one and two adjacent object facts. It checks exact and one-under byte/item ceilings, mapped
peaks, configured precedence, empty cleanup and overflow after prior refusal. The existing direct
and deferred inline-style fixture still verifies early allocation admission and configured GPUI
map refusal. Its old peak expected only the positive one-byte/item placeholder; the explicit
startup observation now records the full continuation charge. Run
`4e34c00d-add7-4060-88fb-be6989c8df22` passed 222 of 223 tests and exposed that stale expectation;
the fixture formula now includes the continuation's struct and five semantic items for its
before-first-object prior position, preserving exact-fit and one-under checks.

Independent resource review accepted this boundary. Final LLVM, one-job, nonincremental,
debug-zero nextest run `3888c235-a702-49de-8964-6895ca653c26` passed all 223 tests across
`range_widget`, `exact_geometry` and `prepublication`, with no skips, in 20.297 seconds.

## Mapped GPUI Remaining Capacity

On 2026-09-28, observed preparation stopped deriving the GPUI allowance by subtracting raw
occupancy from the minimum raw ceiling. After the existing positive byte/item reservation,
`CapacityObservations::remaining_capacity` computes the smaller of configured headroom
`K - R` and enclosing headroom `H - (S + (R - B))`, using checked arithmetic in each dimension.
No prospective display credit enters this startup allowance. Unobserved admission keeps its
existing raw ceiling path; production zero baselines retain identity behavior. This does not
enable session baselines or establish shared-output GPUI admission.

Tests qualify higher and lower enclosing baselines, configured and enclosing limiting dimensions,
maximal admitted output plus one-byte/item overflow, explicit refusal precedence and independent
peaks, baseline underflow (including one below baseline), and maximum representable arithmetic.
Independent source review found no scoped blocker. LLVM, one-job, nonincremental, debug-zero
nextest run `386baa9c-87ae-4526-b6e6-fe59f91df94f` passed all 221 tests across `range_widget`,
`exact_geometry` and `prepublication`, with no skips, in 20.647 seconds.

## Enclosing Preparation Result Evidence

Prepared responses and `ExactGeometryFailure` now retain an optional enclosing byte/item peak
copied from the observer. Crate-private accessors keep it available for session integration without
changing existing raw geometry peak getters. Validation and direct-admission paths without an
observer report `None`; arithmetic failures after observer creation retain its prior representable
peaks, including `Some((0, 0))` before a first valid observation. This evidence never supplies
refusal attribution for arithmetic or GPUI errors. Nested success and failure use the restored
observer, and commit still reports the existing raw geometry evidence.

The added inline metadata increased the large shared-presentation fixture's charged peak by 16
bytes, from 903568 to 903584 on the supported target. Both ordinary widget publication wrappers
embed `PreparedTargetResponse` and already admit their `size_of` in `response_preparation.rs` and
`response_commit.rs`. Initial nextest run `fc6aab3f-7501-460d-a66d-d3bc4dca0309` passed 218 tests
and failed only that fixed snapshot. The snapshot was updated after tracing the new metadata
charge; the dynamic exact-fit, one-byte-under and cleanup assertions remain intact.

Integration probes cover independent raw/enclosing peaks after nested refusal and arithmetic
failure, cloned failure evidence, pre-observation overflow, stale response validation, and ordinary
and resident prepared text/object success and refusal. Session consumption, sharing discovery,
GPUI startup/output allowances and retry routing remain pending.

Independent review accepted the result plumbing and metadata accounting, subject to the corrected
snapshot passing. Final LLVM, one-job, nonincremental, debug-zero nextest run
`30b7a215-fced-4598-a981-95c21da28e91` passed all 219 tests across `range_widget`, `exact_geometry`
and `prepublication`, with no skips, in 20.975 seconds, satisfying that condition.

## Checked Enclosing Observation Mapping

On 2026-09-28, `CapacityObservations` gained checked preparation and enclosing baselines.
Each observation maps raw total `T` to `S + (T - B) - C` independently for bytes and items,
rejecting a raw total below its baseline or incremental credit larger than growth. The checked
addition precedes subtraction, so an overflowing intermediate remains terminal even when credit
could make the mathematical result representable. Mapping failure clears refusal attribution and
preserves prior representable observer peaks. Configured admission continues to compare raw `T`.

Ordinary and nested preparation now use this mapping entry point. The baseline fields follow the
existing observer move/restore path into nested preparation. Production constructors still select
zero baselines and callers supply zero credit, preserving identity mapping. Session baseline
derivation, live allocation credit discovery, GPUI startup/output separation, outward enclosing
peak reporting and retry routing remain pending; this primitive does not establish host admission.

The integration probe covers changing credits with noncoincident maxima, configured precedence,
exact and one-under byte/item limits, baseline underflow, credit exceeding growth, addition overflow,
maximum representable values and nested baseline preservation. LLVM, one-job, nonincremental,
debug-zero nextest run `2537d7d3-eb6e-46f8-a9ad-3b5bb9749a55` passed all 218 tests across
`range_widget`, `exact_geometry` and `prepublication`, with no skips, in 21.286 seconds.
Independent resource review accepted the mapping and its explicitly limited production integration.

## Nested Preparation Observations

On 2026-09-28, nested target preparation began carrying the response's existing capacity
observer through each checked transition admission. Publication moves it only after fallible
base arithmetic succeeds and restores it before mapping either success or failure. This replaces
the aggregate peak replay described below. Earlier configured/enclosing peaks survive; representable
refusals use configured precedence. Arithmetic failures clear attribution and retain saturated raw
attempt evidence separately from the observer's representable peaks. Standalone transitions retain
their existing configured-only admission.

Both production views still receive identical raw charges. Observation-specific shared credits,
GPUI startup/output separation, session baseline derivation, enclosing peak reporting and retry
routing remain pending. The feature-only probe qualifies nested exact byte/item limits, mixed
refusal precedence, overflow after refusal, earlier distinct peaks and zero capacity. Existing
source-backed transition fixtures now require `test-support` for their observer type; independent
review identified the missing module feature gate and it was corrected.

Independent resource review accepted the scoped production trace and corrected feature gate.
The ordinary `exact_geometry` target passed `cargo check` without `test-support`. Final LLVM,
one-job, nonincremental, debug-zero nextest run `8074ecac-6bbc-4ff9-af6d-3a8b667d849c` passed all
215 tests across `range_widget`, `exact_geometry` and `prepublication`, with no skips, in 20.309
seconds. The earlier run `2fbf1ce8-9c0a-42e8-bbd3-ccb43652ba69` also passed 215 tests before the
feature-gate correction. GPUI, dependency pins and host-session routing are unchanged.

## Separate Preparation Capacity Observations

On 2026-09-28, immutable preparation admission began using `CapacityObservations` in the widget
fork's `src/range_geometry/exact/capacity_observation.rs`. Each observation independently checks
configured and enclosing byte/item limits and retains each view's high-water marks. A simultaneous
failure gives configured refusal precedence. Arithmetic failure and GPUI errors remain unattributed.

Production callers currently supply identical raw charges to both views; their decisions remain
equivalent to the previous minimum ceilings. Positive remaining-capacity reservations and nested
successor admission still enforce those minima. The nested publication path imports aggregate raw
peaks into both views, clears any classification from that aggregate, and replays only exact nested
refusal evidence. This is valid only while the two views are identical: sharing propagation must
replace this import with each view's actual observations. Separate internal peaks do not yet supply
host-session peak reporting, session budget derivation, sharing credits, or retry routing.

The integration probe qualifies differing observations, noncoincident high-water marks, exact and
one-under byte/item limits, mixed-limit configured precedence, zero and maximum representable
counts, and clearing refusal after successful admission. Existing preparation tests continue to
cover positive reservations, arithmetic failure, immutable retry and cleanup. Independent resource
review found no scoped blocker. LLVM, one-job, nonincremental, debug-zero nextest run
`84265f82-ee61-4e75-95fa-8385c8259c8a` passed all 214 tests across `range_widget`, `exact_geometry`
and `prepublication`, with no skips, in 20.151 seconds. No GPUI API or dependency pin changed.

## Shared Presentation Preparation Peak Map

On 2026-09-28, source inspection qualified why the existing final-retained overlap query cannot
simply be subtracted from every preparation peak. This is implementation evidence for the existing
widget prepublication and shared-resource contracts, not new design authority. Production host
ceiling propagation remains unimplemented.

The enclosing session and configured geometry budgets need distinct charge views. The configured
view must preserve existing component-limit semantics. The enclosing view counts resident backing
once, with credit only for an allocation already charged by another live owner. A final overlap
amount cannot be applied retroactively to earlier peaks; nor can credits raise configured limits.
Arithmetic failure remains terminal and cannot be attributed as an enclosing refusal.

For an observation, let `S` be current session charge, `B` the full geometry-owner plus borrowed-input
baseline included by preparation, and `T` its raw total including that baseline. The enclosing
charge is `S + (T - B) - C`, where `C` covers only duplicated display charges introduced by this
preparation observation. Current session overlap is already reflected in `S`; subtracting it again
would undercount. Derive byte and item views separately using checked arithmetic. This expression
is an accounting identity, not permission to subtract a final credit from a peak: enclosing high
water must be observed per boundary, because the maximum raw total and maximum shared credit need
not occur together. Existing raw peaks remain evidence for configured geometry limits.

The relevant source paths in the sibling widget checkout are:

- `src/range_widget/prepublication/session/accounting.rs::current_charge` already subtracts current
  geometry presentation overlap against resident object pages and removes inline nested owner
  records from recursive charges. Geometry preparation instead starts with the full geometry owner
  plus borrowed response payloads. Session derivation must reconcile these different baselines,
  including both text and object input for an object response; it cannot subtract geometry counts
  blindly from the session total or charge borrowed resident input a second time.
- `src/range_geometry/exact/prepared_admission.rs::admit_response_continuation` admits a copy before
  allocating it. `copy_response_continuation` shares deferred display backing but copies fallback
  text and owns a new deferred record. Display credit must apply at that pre-copy observation as
  well as afterward. Ordinary fallback strings, records and semantic items remain fully charged.
- `src/range_geometry/exact/scan.rs::process_object_page` takes a deferred object out of scanner state
  and temporarily adds its full charge to fixed custody while admitting it. An overlap query over
  the scanner no longer sees that object during this interval. Credit must follow the detached
  caller custody and be restored or removed with it, including on failure. Deferred-tail creation
  likewise needs the prospective shared display represented before cloning.
- `src/range_geometry/exact/scan/output.rs::admit_layout` observes returned GPUI output and reserves
  scanner fragment storage before `scan.rs::admit_inline_object` creates its matching widget
  presentation metadata. Therefore scanning only `object_presentations` misses a real shared
  display during these observations. The caller already knows the inline display allocation;
  accounting must carry that identity through returned-output and fragment-retention observations.
  Discarded index output still coexists with its resident input before it is dropped.
- `src/range_geometry/exact/scan/output/reservation.rs::binding` sets GPUI's retained allowance
  before output exists. Prospective display credit therefore also belongs at this boundary; adding
  credit only after GPUI returns is too late to admit an exact-fit host budget. Any allowance for
  shared inline display must be specific to that call's known input, preserve configured GPUI
  limits, and be reconciled with returned charge before further allocation. Ordinary text,
  oversize style presentation and line finalization cannot inherit resident-inline display credit.
  GPUI's `streaming_layout.rs::resume_streaming_layout_session` validates continuation capacity
  before shaping, when inline display is not part of the charge. An enlarged total ceiling cannot
  by itself prove that this earlier check fits: prospective display credit must not subsidize a
  continuation-only observation. Startup and output checks need separate qualification before
  enabling the credit; post-return checking alone does not prove admission before allocation.
- `src/range_geometry/exact/prepared_admission/publication.rs` and `target_arrays.rs` retain the
  unchanged current owner, scanner delta, destination storage and publication conversion together.
  Existing shared-output payload deductions must be reconciled with resident display credits;
  do not deduct the same charge twice. Credit follows charged occurrences, not merely the number
  of matching page references. Index completion also transfers the enclosing observation into
  `transition::PreparationCapacity` while preparing its target successor.

GPUI evidence is in the sibling owned fork's
`crates/gpui/src/text_system/streaming_layout/inline.rs`: `prepare_inline` shares presentation text
with its shaped line, while `inline_charge` includes the display length in fragment bytes. Thus the
display is already present in returned-output charge before widget metadata exists. The style-run
reservation accepted separately remains necessary; runs and display backing are different owners.

The existing bounded pointer-and-length identity comparisons can identify shared resident backing
without retaining another object registry. A prospective credit must be bounded by the display
charge actually included in that observation. Equal text in an independently cloned allocation
does not establish shared ownership. Empty displays contribute zero bytes; duplicate references to
one resident page do not multiply credit. Matching allocations must remain live during comparison.

Verification must cover direct and deferred objects, index output discarded after admission and
target output retained before metadata insertion, continuation pre-copy, detached-deferred error
paths, and terminal publication/nested successor peaks. Exercise empty and nonempty display,
independent equal-content allocations, repeated page references, exact fit and byte/item one-under,
configured-versus-enclosing precedence, repeated retry with stable successor IDs, and cancellation
cleanup. The shared-deferred fixture establishes retained overlap only; the existing 213-test result
does not establish these prospective-credit or session-host-ceiling guarantees.

The GPUI reservation still uses one retained ceiling and its layout failures have no enclosing
attribution. Shared credit must not turn such an error into an inferred retryable refusal, and a
second shaping attempt is not acceptable evidence. The remaining implementation must preserve
explicit attribution across reservation and nested successor boundaries before session retry routing
is enabled. This map does not establish that existing GPUI error attribution is sufficient for all
host-ceiling outcomes.

Independent resource review accepted this evidence boundary after correcting the deferred-function
citation and explicitly separating GPUI startup admission from output admission. The root verified
the cited startup code and response-input baselines. No production source, dependency pins or test
expectations changed; no runtime tests were rerun for this source-inspection-only qualification.
Full host-budget propagation and phase 747 remain pending.

## Transient Inline Style Run Coexistence

On 2026-09-28, inspection found that inline-object and oversize style-run allocation guards did
not carry those buffers into the remaining capacity passed to GPUI. GPUI borrows their runs while
shaping and keeps the input vectors alive through returned-output construction, but its inline
output charge omits those vectors. The widget now subtracts checked run bytes/items before binding
construction and records their coexistence with returned output. Later scanner growth excludes the
already dropped runs. Ordinary text transfers runs into charged output and supplies no extra charge.
This corrects the reservation omission left outside the previous attribution-only acceptance.

The inline fixture now covers empty/nonempty presentation and direct/deferred input at its earlier
allocation guard and the positive reservation boundary, including byte/item one-under and exact
cleanup. Existing oversize exact-fit/shortage tests and the integration suite cover ordinary scans.
Source inspection establishes the common index/target call path and GPUI ownership distinction;
this is not a claim of complete shared presentation or session host-budget accounting.

Initial integration run `f3f5bf39-06d7-4477-b683-e5b0f14af460` passed 213 tests. Extending the fixture
initially assumed the minimum positive reservation reached shaping's maps check; run
`d5d32afc-7ea9-422b-a3cf-23ae30b00a82` passed 212 and failed that assertion because GPUI session
startup can reject the tiny allowance first. The test now distinguishes GPUI entry from scanner
refusal without requiring a later failure point. Focused run `51f67cbb-acf9-440e-b69e-a8616f1caf2a`
passed; final run `2df59c87-9171-4d38-9ba3-606c5b6b60ed` passed all 213 tests, zero skipped,
in 19.381 seconds. Independent resource review accepted the change. The five older baseline unit
failures were not rerun or waived, and canonical dependency pins remain unchanged.

## Pre-Shaping Reservation Refusal Attribution

On 2026-09-28, the existing requirement for a positive byte and item remainder before GPUI
binding construction was routed through explicit count admission. Checked occupied-plus-one counts
now contribute to attempted peaks and configured/enclosing refusal attribution. Allowed remainders
are unchanged. Arithmetic overflow remains unclassified; success clears refusal evidence before
GPUI, whose ambiguous failures remain unclassified. This does not implement session retry routing,
shared preparation credits or full preparation protection.

The feature-gated probe exercises the production reservation helper for exact positive remainder,
zero remainder, independent dimensions, mixed configured/enclosing precedence and overflow.
Binding construction order and GPUI non-attribution are inspection evidence. The existing inline
style fixture still distinguishes its earlier allocation guard from entry into GPUI.

Initial full run `ba174982-e66c-43e3-83ca-2aa29364fe8f` passed 212 of 213 tests; the inline style
fixture expected the older peak without the reservation floor. Its correction keeps the earlier
style-guard assertions and adds the fragment record, positive remainder and composite continuation
growth only when that later reservation is reached. Focused run
`b241fb9a-5e31-4d9a-acad-6362036ddbd0` caught double counting the temporary style run in that formula;
run `1ed13eb4-eab3-4a1a-ab66-ac7704e8e6d8` passed after aligning it with the reservation counts.

Independent review accepted the boundary and corrected fixture. Final full integration run
`77c3cbf7-7272-44ea-87ff-caa69c792254` passed all 213 tests, zero skipped, in 20.743 seconds.
The five older baseline unit failures were not rerun or waived; no canonical dependency pin changed.

## Explicit Preparation Refusal Attribution

On 2026-09-28, widget explicit preparation admission began retaining the exact byte/item pair
rejected by a limit comparison, separately from historical peaks. Internal typed attribution
reports enclosing-only refusal only when both counts fit configured geometry limits; either
configured dimension takes precedence. Nested successor preparation carries that evidence into
the enclosing failure. Arithmetic, component, validation and GPUI failures without an explicit
comparison remain unclassified. No production session retry routing changes in this boundary.

The integration ceiling fixture verifies both mixed-limit orientations, byte/item exact, under
and zero ceilings, resident/external text and object preparation, immutable custody and retry.
The nested capacity integration fixture verifies exact refusal counts and clearing on success
or arithmetic overflow. Nested propagation itself is inspection evidence; the response fixture
does not isolate a publication-stage nested refusal. Independent resource review accepted this
bounded evidence and found no false retry attribution.

Focused nextest run `a354d081-904f-41cc-a94a-5d29757e3f55` passed. Final full integration run
`0c84737c-9379-4d26-96cb-0d1e1e832c2f` passed all 212 tests, zero skipped, in 19.100 seconds.
The first build caught a test budget initializer missing the new field; it was corrected.
Run `07268b07-09c6-42fb-80ea-46fe499ca138` rejected the test's overbroad expectation that every
host-limited failure has explicit attribution: a one-under response reached GPUI's ambiguous
`Layout(CapacityExceeded(Total))`. The regression now requires that error to remain unclassified.
Run `37b7af5c-f92c-498b-bcd6-d37861be99f5` disproved a publication-stage coverage assumption in
that fixture; the unsupported assertion was removed and the inspection evidence is stated above.

Shared presentation credits throughout preparation peaks, GPUI refusal provenance, session budget
derivation/routing and complete phase 747 remain pending. Existing five baseline unit failures
were not rerun or waived. No canonical dependency pin changed.

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

## Admitted Response Successor Identity

Preparation previously allocated three successor identities on every attempt. Before enabling host
preparation retries, those identities now enter admitted response custody once for all four delivered
and resident text/object paths. Preparation reads them immutably. A checked complete reservation
precedes counter mutation, and commit, cancellation or terminal failure clears the retained state.
The larger inline state is covered by the existing session-size ownership charge.

Independent lifecycle/resource review accepted this boundary. Focused integration run
`88acb3fd-1b12-42f7-85ab-83daafeb3be3` passed both admitted-response tests; full run
`f026b0aa-b017-4ca0-beb1-f1ae15e4a9eb` passed all 211 integration tests in 21.611 seconds,
zero skipped. Tests verify stable identities across repeated byte/item current-capacity denial,
fresh subsequent reservations, resident and delivered progress to Ready, cleared reservations after
commit/cancellation/collision, and exact cleanup. Overflow atomicity was verified by inspection.
These tests do not yet exercise typed preparation retries: session budget derivation, shared
presentation accounting and typed refusal attribution remain outstanding. The earlier five baseline
unit failures were neither rerun nor changed by this boundary.

## Deferred Presentation Sharing

Inspection before session-budget derivation found that deferred scanner facts and prepared
continuation copies used ordinary source-fact cloning. That deliberately duplicates presentation
backing for source response custody, but contradicts the geometry sharing contract. Geometry now
uses a private clone that copies fallback text and aliases immutable presentation backing. Ordinary
fact and object-page clones retain independent backing. Current and prepared geometry overlap
queries include deferred aliases by allocation identity and extent; fixed records, boxes, fallback
copies and semantic items remain charged. Repeated page references do not multiply overlap credit.

The integration regression covers index and target scans, repeated immutable preparation, deferred
commit, the next deferred response and publication of the prior deferred object's presentation.
Independent source clones receive no overlap credit. Focused run
`afb66fbf-8f16-42f5-94e6-c512fe35667f` passed; complete run
`08425d4e-6f2c-47f0-a1e4-563435fd4034` passed all 212 integration tests in 19.238 seconds,
zero skipped. Independent resource review accepted this boundary. The final fixture keeps the
complete index and partial target responses on the same object population. Initial fixture
attempts exceeded the fixture's fragment or baseline limits and used
an empty source whose target was already complete; the final fixture uses valid presentation
metrics and a nonempty source to exercise an active target.

This boundary does not propagate shared credits through scanner preparation peaks or implement
session host ceilings and typed retryable refusal. Those remain required before combined resident
reservation can rely on the session. The five earlier baseline unit failures were not rerun or
changed.
## Text Response Deferred Custody Is Unreachable

The pending plan proposed carrying deferred display credit through text-response publication.
Source inspection invalidated its reachability premise: `scan::process_object_page` creates a
deferred tail only for an incomplete object page, which returns before text scanning and requests
another object page. On the next response it takes and admits the deferred object. A complete
object page rejects any remaining deferred custody before `process_page_range` can request context
or proceed to forward text. Inline admission returns ordinary success/error, not a context request.
Initial and terminal-successor jobs start without deferred custody. Text continuation copies have
empty output collections, so original retained output does not create incremental copy credit.

Qualify that invariant instead of adding unreachable credit plumbing. The integration fixture
`text_successor_retains_shared_output_and_consumes_deferred_custody` uses actual incomplete/complete
prepared object responses for index and target jobs with empty and multibyte display. It checks
unchanged original custody, zero deferred ownership after commit, and following delivered/resident
text preparation against independently mapped raw peaks, exact fit, byte/item one-under and retry.
Context reachability is a source-review obligation; this fixture does not exercise backward context.
Source-finalization, terminal/nested publication and session-budget integration remain pending.

LLVM, one-job, nonincremental, debug-zero integration run
`816b92a9-13e6-4cd4-92ad-7e880ce4f01f` passed all 227 tests across `range_widget`,
`exact_geometry` and `prepublication`, with no skips, in 21.222 seconds. The focused regression
also passed (`6795f14c-8bb0-4467-a657-4fa2a4399642`). No production behavior changed.
Independent semantic/resource review found no blocking issues. It traced direct/prepared admission,
public text-request guards, scanner origin/checkpoint constructors, terminal successor creation and
failure cleanup. No alternate text path retains deferred custody. No source or test changes followed
the successful full run. Root phase 807 and widget phase 62 remove the obsolete credit obligation.

## Terminal Index Publication Has No Retained Display Credit

Terminal index publication does not need the retained-output credit introduced for targets.
`scan::output::admit_layout` retains fragments only for target jobs; index output remains charged
while returned custody exists, then drops that custody and restores its transient credit. Complete
object responses consume deferred custody before publication. Index checkpoint arrays carry no
presentation payload. Nested target preparation either uses `Scanner::from_checkpoint` with empty
output/deferred state or creates an empty terminal publication. The existing observer moves into
nested preparation and returns before error mapping and cleanup admission. These boundaries have
zero incremental shared display to deduct; original owner and borrowed inputs remain in the baseline.
No production change or additional credit plumbing is required.

The integration regression `terminal_index_publication_has_no_retained_display_credit` exercises
whole response preparation for initial and prior-deferred index responses, empty and multibyte
presentation display, resident and delivered admission, and active versus immediately complete
nested targets. It compares raw preparation peaks with checked baseline mapping at zero and elevated
current charges, then verifies exact fit twice, byte/item one-under refusal, empty failure release,
unchanged owner counts and successful retry. Earlier scanner output still receives its scoped credit;
the fixture's terminal peak has zero credit. Its raw peak comes from production accounting, not an
independent allocation oracle. The test preserves attribution for ambiguous GPUI Total failures.

Focused run `576d9fd1-d04c-4403-9e14-dbe4244d1f16` passed. Full LLVM, one-job, nonincremental,
debug-zero run `ada5c0e7-8600-400c-aa2e-5e29587e5ac8` passed all 230 integration tests across
`range_widget`, `exact_geometry` and `prepublication`, zero skipped, in 20.877 seconds. Formatting
and scoped whitespace checks passed after normalizing two mixed line endings; the whitespace-only
correction passed an ignore-line-ending diff check. No semantic test changes followed the full run. The earlier
five baseline unit failures were not rerun or changed. Session budget derivation and retry routing
remain the next integration boundary; this proof does not accept whole-session host admission.

Independent semantic/resource review found no blocking findings after tracing discarded output,
checkpoint content, nested constructors, observer restoration and cleanup admission. Root phase 811
and widget phase 66 accept this no-production-change qualification.

## Session Response Preparation Uses Enclosing Capacity

Phase 812/widget67 connects admitted delivered and resident text/object responses in both geometry
stages to the enclosing preparation observer. The previous ordinary preparation calls checked only
raw geometry limits after the session's current-ownership guard; a fitting current charge did not
constrain incremental preparation. The replacement derives S from checked session current ownership
plus buffered effects and H from the clamped available capacity. Existing geometry/resident display
overlap is removed once by current_charge; the preparation baseline still includes the raw geometry
owner and borrowed pages. Per-observation candidate display credits remain separate.

Prepared success and failure peaks update session high-water evidence. A failure explicitly attributed
to Enclosing and still within configured session capacity reports CapacityBlocked, retains admitted
response and successor IDs, consumes zero work and emits no cleanup or effects. Raw configured limits
remain unchanged. Configured excess stays terminal, and unattributed GPUI errors are not reclassified
as host refusal. Successful preparation commits through the existing response path.

Expanded admitted_response integration cases exercise byte/item availability equal to current
ownership as well as one below current ownership: repeated preparation refusal, cancellation, drop,
exact-key collision, restored capacity and eventual readiness preserve the existing custody and
identity invariants. Whole progression includes delivered and resident responses in index/target
stages. The separate geometry_preparation_peaks_preserve_refusal_attribution fixture measures a new
preparation peak above earlier reservations, accepts its exact byte/item limits and rejects one-under.
It allows terminal DeterministicGeometry for unattributed GPUI refusal; explicit retry is separately
required by the current-equal cases.

Fixture corrections: the default 256 KiB object reservation dominated cumulative high water
(274509 bytes), so it could not serve as this step's exact-capacity oracle. The final fixture uses
coherent resident and pending envelopes of one object/512 bytes and proves the preparation peak is
higher than previous peaks. Resident limits must fit pending limits or environment validation fails.
A configured-surface variant at this small session peak was rejected earlier by the mounted-widget
initial-capacity requirement and was removed; existing cleanup_capacity_cases covers configured
terminal outcomes elsewhere, while classification at this boundary is source-reviewed. A GPUI
one-under refusal can be unattributed and terminal, so the test must not assume every shortage is
explicitly retryable. These fixture corrections did not alter production policy.

Focused expanded retry run 2784e9bc-9707-45d8-9453-c186be16f7a8 passed 11 tests; initial full run
cbf579ff-ca4d-4a43-b1b2-b3d453ff4a55 passed 230 tests in 21.028 seconds. Final focused peak run
7a481e09-f4ed-4b09-844a-273fd00a9337 passed one test with 230 skipped. Independent read-only
semantic/resource review found no blockers, including a second review of the fixture corrections.
The session fixtures deliver empty object pages; shared inline-display composition is source-reviewed
and supported by the prior geometry qualification, not directly exercised in these fixtures.
Candidate-surface, ready and transition preparation remain separate work under phase 747.

Final verification: a4aab56d-0d9e-4dd4-afb4-c167be2e5196 passed all 231 range_widget,
exact_geometry and prepublication tests, zero skipped, in 21.216 seconds. Stable edition-2024
skip-children rustfmt checks and scoped diff checks passed. No manifests, canonical pins, test
support API or temporary processes/directories were added. Existing GPUI float-literal and
proc-macro-error2 warnings remain unchanged.

## Candidate resident transfer buffer admission

Phase 813 / widget phase 68 moves resident text/object destination-slot admission ahead of both
allocations. Checked combined growth includes the current session charge and its already reserved
cleanup record. Configured refusal is terminal; available-capacity refusal completes that temporary
record and leaves resident and geometry owners intact. Actual text capacity is checked with pending
object slots before the second allocation, then both actual capacities are checked before surface
preparation. Attempted peaks feed session high-water evidence, including failure paths.

The two integration cases cover zero, text-only, object-only and combined counts, exact fit,
independent byte/item refusal, repeated denial, configured-terminal classification and arithmetic
overflow. A test-support-only associated probe invokes the production helper. Tests initially placed
in source were relocated to the integration target; review also caught and corrected the module's
missing test-support feature gate. The default-feature prepublication check passed in 3.13 seconds.
Final integration run `0252971b-a642-4e27-b50b-1d0ea31234df` passed 233/233 in 21.162 seconds;
post-gate focused run `fc17e18d-754d-44ab-8c11-05d41de75186` passed 2/2 in 0.016 seconds.
Independent semantic/resource review closed without remaining blockers. Allocator over-reservation
and the exact session transfer-refusal cleanup branch are source-reviewed, not fault-injected.
Surface map/object/gap/selection allocations and later ready/adoption admission remain pending.

## Candidate surface page ordering admission

Phase 814 / widget phase 69 extracts page ordering into shared preparation. It checks requested
slots before fallible reservation, actual capacity before filling, and capacity plus length before
box conversion when shrinking could allocate. An in-place unstable sort keyed by source start and
original index preserves the prior stable ordering without allocating sorting scratch. The session
baseline includes reserved cleanup custody, both actual transfer buffers and the prepared surface
record. Configured refusal wins over host refusal; attempted peaks propagate before errors, and
host refusal completes only the temporary cleanup reservation while retaining resident/geometry.

Focused integration run `ade39251-ec63-4434-85b1-75710e7dccce` passed 2 cases. These cover empty,
singleton and duplicate/reordered positions, exact fit, byte/item one-under, repeated host refusal,
retry success, configured precedence and enclosing arithmetic. Full run
`797b4a34-7103-4c78-8c48-a58d209fba2e` passed 235/235, zero skipped, in 21.641 seconds.
The default-feature prepublication check passed in 3.09 seconds. Scoped rustfmt and diff checks
passed; independent semantic/resource review found no blockers. Allocator overreservation,
box-conversion coexistence and exact live-session page-order refusal cleanup were source-reviewed,
not fault-injected. Test probes remain behind test-support. No manifests or canonical pins changed.
Fragment maps, realized object/gap geometry, selection/composition and later transitions remain pending.

## Candidate fragment map collection admission

Phase 815 / widget phase 70 extracts owned map enumeration and collection. The same fragment
variant rules and adjacent-position first-wins deduplication are preserved, including distinct
inline-object gaps. Requested slots are checked before fallible reservation and actual capacity
before filling. The session baseline includes current cleanup custody, actual transfer buffers,
the prepared surface record and retained boxed page ordering. Configured refusal precedes host
refusal, attempted peaks propagate before errors, and host refusal completes only temporary cleanup.
Resident pages and geometry remain in place for retry. Ordinary mounted preparation uses the same
collector with its existing later admission; remaining geometry and transition work stays pending.

An initial compile rejected calling maps() on the fragment enum; explicit per-variant access fixed
that local implementation error. Full integration run `d459b4ca-a593-4210-8621-94f666249a32` passed
237/237 in 21.299 seconds. A final additional test covers differing placements at duplicate logical
positions, distinct object gaps and nonadjacent repeats; focused run
`8de34a1a-5329-47ed-b165-68e5f2aa32ae` passed all three cases in 0.026 seconds (235 skipped).
The other cases cover empty/singleton/duplicate maps, exact fit, independent byte/item one-under,
repeated denial, capacity restoration, configured precedence and checked enclosing arithmetic.
Default-feature prepublication check passed in 3.16 seconds; scoped rustfmt and diff checks passed.
Independent semantic/resource review found no blockers. Allocator overreservation, live-session
map-refusal cleanup and per-variant map extraction are source-reviewed rather than directly
fault-injected by the new helper tests. No manifests or canonical pins changed.
## Candidate Realized Geometry Buffer Admission

Root phase 816 / widget phase 71 admits combined realized-object and object-gap vector storage
before either allocation, actual object capacity before allocating gaps, and both actual capacities
before filling. The enclosing baseline includes retained session/cleanup, transfer buffers, prepared
surface record, page ordering and map capacity. Nonempty or undersized input buffers are rejected by
surface construction rather than allowing implicit growth. Existing object identity, presentation,
ordered-gap and geometry validation remains in the realization path. Host refusal observes peaks,
releases temporary cleanup and preserves resident/geometry custody; configured refusal, allocation
failure and arithmetic remain terminal. Ordinary mounted publication keeps its existing late guard.

The two focused tests cover empty, object-only, gap-only and combined buffers, exact fit, repeated
byte/item one-under refusal, restored availability, configured precedence, count multiplication and
separate enclosing byte/item overflow. Initial focused run `223946b6-40f9-4c84-8ae6-9953accdb3f3`
passed 2 tests. Final full run `f73ac333-cdcc-42a2-bbae-f0897e20d57f` passed all 240 tests in 21.754s.
The default-feature prepublication check passed in 3.30s; scoped formatting and diff checks passed.
Independent semantic/resource review found no blocker. Allocator overreservation, live-session
buffer-refusal cleanup and count/fill equivalence were source-reviewed rather than fault-injected.
Selection/composition collections, boxed conversions and publication transitions remain pending.

## Candidate Highlight Geometry Collection Admission

Root phase 817 / widget phase 72 replaces growing selection/composition vectors with exact-count
preparation. Count and fill use the same immutable rectangle iterators, preserving text wrap pairs,
minimum widths, selected inline-object bounds after text bounds, gap filtering and directed ranges.
Combined requested storage is admitted before either vector allocation, actual selection capacity
before composition allocation, and both actual capacities before filling. The session baseline
includes current cleanup custody, actual transfer buffers, prepared surface record, boxed page
ordering, map capacity and realized object/gap capacity. Configured refusal wins; attempted peaks
propagate before errors and host-only refusal releases temporary cleanup without moving resident
or geometry custody. Ordinary mounted preparation retains its separate later admission boundary.

Three focused tests cover expected wrap rectangles, reversed selection, object-only gap selection,
text-before-object ordering, empty text/composition, exact fit, repeated byte/item one-under refusal,
restored availability, configured precedence and separate enclosing byte/item overflow. Focused
run `a2380f9d-e18d-46fa-9069-24d7c88f687f` passed 3 tests (240 skipped). Full integration run
`3190040b-f4b6-4cff-a30b-cb8d036d430d` passed 243/243 in 21.504s. Default-feature prepublication
check passed in 3.21s; scoped formatting and diff checks passed. Initial missing test-probe type
qualification and test gap-constructor arguments were corrected before passing verification.
Independent semantic/resource review found no blockers. Allocator overreservation and live-session
highlight-refusal cleanup were source-reviewed rather than fault-injected. Boxed conversions and
later publication transitions remain pending. No manifests or canonical dependency pins changed.
## Candidate Surface Box Conversion Admission

Root phase 818 / widget phase 73 admits each realized-object, object-gap, selection and composition
vector conversion before `into_boxed_slice`. Excess capacity requires admission of current live
storage plus destination length; exact-capacity conversion needs no duplicate allocation charge.
After conversion the next check carries the boxed length instead of old vector capacity. The
surface callback now owns realized-buffer charges, while the enclosing session baseline retains
session/cleanup, transfer buffers, prepared record, page order and maps. Conversion checks also
carry caret and placeholder charges. Host-only refusal drops temporary collections and completes
temporary cleanup without moving resident or geometry custody; configured refusal stays terminal.
Ordinary mounted admission and later candidate/Ready/adoption transition guards remain separate.

Three integration cases force excess capacity, qualify sequential release, exact-fit and one-under
byte/item bounds, repeated refusal, configured precedence, empty allocated and unallocated vectors,
exact-capacity vectors, enclosing overflow and refusal after an earlier successful conversion.
Restored availability preserves values and final charges. Initial focused run
`0b06ca0c-cbb8-4d7c-83ad-57373570d8cd` passed two tests before the final third case was added.
Full run `46e73c4a-4c2a-4ac5-bcff-60915348f5f4` passed 246/246 in 21.445s; default-feature
prepublication compilation passed in 3.21s. Scoped formatting and diff checks passed. Independent
semantic/resource review found no blockers. The probe exercises two u64 collections; production's
four collection types and live-session refusal cleanup were source-reviewed. No manifests or
canonical dependency pins changed.

## Prepared surface retained storage

Root phase 819 / widget phase 74 (2026-09-28) separates returned prepared-surface storage from
its earlier preparation peak. `PreparedCoherentRangeSurface::retained_charge` records final boxed
collection lengths plus retained page ordering, caret and placeholder. Scratch maps have dropped
and excess vector capacity has been released before the session uses this charge. The existing
candidate peak remains available to ordinary mounted preparation. Earlier per-allocation session
admissions and high-water observations remain intact; this post-return check is not preallocation
proof and does not accept the later candidate/Ready/adoption custody transitions.

The new integration case qualifies preservation of non-collection storage while boxing releases
spare capacity. Actual prepared-field construction and session consumption are source-reviewed.
The extra two-usize charge field enlarges the prepared record by 16 bytes on the supported target;
the large shared-object exact-budget fixture changed from 903584 to 903600 after the first full
run caught that expected layout change (246/247 passed). Its exact-fit/one-under checks remain.
Final LLVM one-job integration run `cc0470c9-2e74-49ce-8d03-ea712dbd31f7` passed 247/247,
zero skipped, 24.366s. Default-feature prepublication check passed in 3.23s; scoped rustfmt and
Git whitespace checks passed. Independent semantic/resource review found no blocker. Existing
GPUI float-fallback and proc-macro future-compatibility warnings are unchanged. No manifest or
canonical dependency pin changed; no task-owned temporary resources remain.

## Surface residual admission before highlights

Root phase 820 / widget phase 75 (2026-09-28) includes caret and retained placeholder charges
before highlight vector reservation, instead of first including them during boxed conversion.
The immutable caret lookup and placeholder selection now precede highlight preparation; every
requested and actual vector-capacity observation includes their charge plus realized buffers.
Conversion and final retained accounting still include each residual once. This does not accept
placeholder ownership before surface preparation or later candidate/Ready/adoption transitions.

Full integration run `e166656e-024f-4eb5-91ca-30cbbecad7c5` passed 247/247, zero skipped, 22.442s;
default-feature prepublication compilation passed in 3.24s. Scoped formatting and whitespace
checks passed. Independent semantic/resource review found no blocker. Existing helper tests cover
exact-fit/one-under bytes and items, repeated refusal and configured precedence; actual surface
residual derivation and residual-induced early refusal are source-reviewed, not directly tested.
No manifest, canonical dependency pin or final retained charge changed. Existing compiler warnings
are unchanged; no task-owned temporary resources remain.

## Candidate Ownership Admission Before Transfer

Phase 821 / widget 76 moves standalone candidate accounting and admission ahead of resident-page,
target and cleanup custody transfer. The geometry projection uses the ordinary accounting inputs
with no target and subtracts only active presentation overlap. Transfer spare slots use immutable
resident counts. Configured failure remains terminal; host refusal completes the temporary cleanup
record and leaves resident/geometry/custody owners available for retry. Candidate attempted charge
contributes to high water before either capacity check.

Verification: full integration run `ececa705-7ff6-4ce7-9b03-dca7c9d8ec3b`, 247/247 passed,
0 skipped, 20.873s; default-feature prepublication check passed in 3.07s. Terminal target tests compare
the projection with actual removal across prior-output and empty/nonempty inline display cases.
The added active/deferred projection assertion passed in focused run
`a33c6596-aa00-4582-8acf-dbd3513a1b2b` (1 passed, 246 skipped, 0.019s). Formatting and diff checks
passed. Independent semantic/resource review found no blockers. Live-session refusal at this exact
candidate boundary is source-reviewed, not directly exercised; no additional test was required for
this bounded move. Ready/adoption coexistence remains pending; this does not accept overall phase747.
## Ready And Adoption Admission Before Transfer

Phase 822 / widget 77 shares session shell accounting between current ownership and the immutable
post-candidate projection. The projection adds newly vacant resident-buffer slots and excludes only
cleanup records promoted with the candidate; moved geometry, resident payload and custody storage
are charged through the candidate. Ready and adoption charges are checked and observed before any
page/target/cleanup transfer, with configured failure before host refusal. Host refusal completes the
temporary cleanup reservation and retains retryable ownership. Successful candidate fields receive
those admitted charges directly. Candidate Debug includes content-free origin-session charge.

The common integration driver compares projected origin with actual ownership immediately after
taking the candidate. Initial run `529380c4-6a15-49f8-a76f-4ca326503c69` passed 244/247: three new
assertions incorrectly compared against ownership after cleanup draining (416-byte record multiples).
Moving the assertion before the drain compares the same ownership moment; no production workaround
was needed. Final run `869d3c69-2f31-47e4-9aeb-25b8147e41d1` passed 248/248, 0 skipped, 21.342s.
The additional live test verifies one-byte/item shortfalls, repeated refusal preserving ownership
and cleanup counts, exact-capacity resumption and complete release. It may refuse at an earlier
preparation gate; exact Ready/adoption host refusal and configured precedence remain source-reviewed.
Default-feature check passed after the final Debug addition in 3.36s; formatting/diff checks passed.
Independent semantic/resource review found no blockers. This accepts transition admission only;
placeholder timing and enclosing effect/transition coexistence still need overall preparation audit.
## Candidate Effect-Buffer Isolation

Phase 823 / widget 78 audits the remaining buffered-effect concern without changing production
accounting. `service` creates an empty EffectBuffer per call. The five push sites (validation,
restoration text/object, geometry text/object) each install waiting state, with no delivered or
admitted geometry response, and immediately return. The service loop then breaks. Delivery is
external to that call. Resident and admitted-response progression do not append effects. Therefore
both candidate entries, an existing target in `advance_geometry` and TargetComplete in
`commit_geometry_response`, run with zero buffered effects. Failure and resident release mark the
separate cleanup ledger; they do not append service effects. This proof depends on the present
request-and-wait scheduling and must be revisited if service gains batching or synchronous delivery.

The common live driver asserts a Ready service result contains no effects before processing it.
Since EffectBuffer is append-only until return, earlier effects cannot be hidden. Prepublication
integration run `d98489ce-9d6b-4164-b8e0-c4dc5250eaba` passed 43/43, 0 skipped, 15.220s. Formatting
and diff checks passed. Independent resource review found no blocker. Blocked and failed candidate
attempts are source-reviewed through the same scheduling invariant, not directly instrumented.
No production source, manifest or dependency pin changed. Placeholder ownership and transfer
coexistence remain pending; this finding does not accept overall preparation capacity.

## Candidate Configuration And Placeholder Ownership

Phase 824 / widget 79 initially considered a no-change clone audit: SharedString, font features,
font fallbacks and the settlement coordinator share backing. Independent review found the omitted
constituent: StreamingGeometryStyle also owns StreamingOversizePresentation.runs, a Vec<TextRun>.
The derived configuration clone in finish_candidate copied this vector before any preparation
admission. Shared fields alone therefore did not establish that the full configuration was cheap.

Candidate preparation now clones the environment Arc and borrows its immutable configuration.
This removes that allocation rather than estimating or reserving another copy. The local environment
handle keeps configuration alive across mutable session operations and all refusal paths. This is a
local ownership correction; startup's separate configuration/style clones and full environment
accounting are not qualified by it and remain in the overall preparation audit.

The remaining placeholder clone is SharedString over ArcCow: static backing copies a reference,
owned backing clones its Arc. Neither duplicates string bytes or registers another cleanup record.
The prepared surface record is admitted before the clone enters surface preparation. For an empty
source and nonempty placeholder, surface preparation charges sizeof(SharedString) plus text length
and one item before highlight allocation. The first highlight admission runs even with zero
rectangles. That conservative charge remains in conversion and retained-surface accounting.
Nonempty sources or empty placeholders discard the temporary handle; refusal drops local handles
without dropping environment-owned backing. No placeholder text copy or shaping happens here.

Evidence inspected: range_widget/prepublication/session/candidate.rs finish_candidate;
prepublication/types.rs environment owner; range_widget/types.rs configuration/coordinator;
range_geometry/exact/types.rs oversize/style; surface.rs prepare and surface/highlight_geometry.rs;
GPUI shared_string.rs, util/arc_cow.rs and font feature/fallback definitions. The allocation removal
and static/owned backing proof are source-reviewed, not allocator-instrumented. Existing live tests
exercise candidate preparation and repeated capacity refusal. Run
8cb7566b-39e5-4264-a360-145b23290e27 passed 43/43 prepublication tests, zero skipped, 15.524s;
default-feature check passed in 3.24s. Changed-file rustfmt and diff checks passed. A broad format
check reported pre-existing differences in lib.rs and exact_geometry tests, which remain untouched.
Independent resource review accepted the correction. No manifest or canonical dependency pin
changed. Transfer coexistence and the wider preparation boundary remain pending.

## Startup Configuration Ownership

Phase 825 / widget 80 removes the full configuration clone in
RangePrepublicationSession::new_with_admission_capacity. Like candidate preparation, that clone
copied StreamingOversizePresentation.runs before admission. Startup now borrows the environment's
configuration and stores a cloned environment Arc in the session. No configuration payload is
copied by this change. The original environment keeps the borrow alive during construction;
on success its temporary handle drops, leaving session custody. Early errors drop temporary
owners, and initial-capacity refusal drops the constructed session without issuing requests.
Validation order, generation allocation and configured/enclosing limit selection are unchanged.

The geometry owner's own style clone is still present. This correction does not qualify that
allocation, the custody-vector reservations, shared environment backing or transfer coexistence.
Those remain required by the overall preparation audit before preserved-resident reservation.

Independent resource review inspected construction, environment ownership and geometry/drop paths
and found no blocker. Allocation removal is source-verified rather than allocator-instrumented.
Prepublication run fc05035b-6bc1-4fec-a082-5f5d84a80330 passed 43/43, zero skipped, 15.808s.
Default-feature compilation passed in 3.31s; changed-file rustfmt and diff checks passed.
No manifest or canonical dependency pin changed.
