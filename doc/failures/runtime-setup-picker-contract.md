# Runtime Setup Picker Contract

## Fixed Footprint Allocation

Review of the setup draft on 2026-10-08 found that the primary viewport's default role heights
did not fit the widget's fixed footprint together with its header, search, headings, runtime
viewport, Add runtime command, footer, padding, border and inter-region gaps. The confirmed
variant exceeded the outer height; clipping would make declared controls unreachable.

The [widget contract](../gui/widgets/thread-root-picker/spec.md#layout) now defines the primary
viewport as the remaining inner allocation. Its immediate and confirmed default role heights
are corrected to match that formula. Qualification must check actual GPUI bounds for both
variants rather than assuming that individually specified heights fit together.

## Visited Scope Retention

The archived draft stores focus and scroll facts in an unbounded vector indexed by every visited
collection key. The widget contract promises restoration for every visited collection, while
the app package requires bounded retained state. Neither the runtime registry nor visited
scope identities have a supported fixed maximum; bounding page residency alone does not bound
this vector.

The proposed correction retained the 16 most recently visited scopes and initialized older
evicted scopes from owner-supplied initial facts. It preserved conversation selection and the
window's remembered new-thread runtime/root. This changed the restoration guarantee and awaited
Operator resolution before authority or dependent implementation changes. The Operator approved
the 16-scope limit after its meaning and reset behavior were explained. The widget contract now
includes the current scope in that limit and requires recency refresh on return. Implementation
and final acceptance qualification have completed against that authority. Durable scope-state
storage would require a separately designed ownership and lifecycle boundary; it is not an
implicit workaround for this GUI phase.

## Search Input Qualification

Picker run `891a24ed-4c7f-49c5-a6e5-7197084c68fe` exposed an invalid input fixture:
`TextInput::set_text` silently resets text and does not emit the change event that the picker
uses to preserve its query during pending admission. A direct reset therefore bypasses the
boundary being tested. Use focused virtual `simulate_input` for user search behavior. Corrected
run `105d1e90-0604-4fcd-9fd5-83e4e4676f82` passed all 22 picker cases, including pending search
and visited-scope restoration. This qualifies the picker checkpoint, not the complete setup mount
or the unresolved scope-retention policy.

## Original Command And Page Custody

Independent review of the healthy shell checkpoint found that later committed admission released
its original outcome and selection lease before asynchronous paired runtime/root publication.
The consumer must retain that original flight through the coherent GUI election. A failed page
read must expose only the exact named read retry while mutation resubmission stays fenced; this
also applies to a typed Existing result awaiting refresh. These corrections remain unaccepted
until the complete consumer qualification passes.

A cancelled old-query page delivery also discarded the current query's staged pair. Obsolete
deliveries must settle only their original request and return before touching current staging.
The delayed-delivery fixture must genuinely hold that old request to qualify the guarantee.

The 25-case consumer checkpoint passed 17 cases and failed eight. Four native cancellation/error
cases exposed a persistent pending command: the picker wrote its temporary in-flight state into
the base command and then cleared only the override. Ending the original command must restore
its base eligibility and focus; terminal owner state must remain effective. Remaining failures
cover delayed query delivery, scoped query preservation and shutdown qualification. The raw bounded
log is `.tmp/runtime-setup-mount-evidence/consumer-checkpoint-tests.log`; it is failed checkpoint
evidence, not mounting acceptance.

## New Thread Scope Search

A source review initially treated clearing the old query before scope replacement as lost query
restoration. The feature contract explicitly requires every New Thread runtime scope change to
clear search; the reusable widget's visited-scope obligation concerns focus and scroll. Restoring
a prior query in this consumer would violate that feature contract. Scope-return qualification
must instead prove that picker and page-owner queries remain empty and coherent, while preserving
the required focus/scroll facts and only eligible pending selection.

## Corrected Consumer Qualification

The setup-service election fixture initially seeded provider turns outside app execution
custody; shutdown correctly refused them with UnprovenExecution. A real pristine CreateThread
mutation supplies the required stale-observation witness without unrelated provider work.
Staged-close fixtures must also retire the original setup service and dispose its unpublished
first candidate before generic shutdown; the actual lifecycle correctly refuses Close/Exit
while the original selection lease is held.

The intermediate 53-case run `5078fa72-51ee-4afe-b450-07d9b26f4e26` passed 51 cases.
Its two remaining failures were invalid fixture assumptions: a blocking delivery barrier on
GPUI's deterministic background executor, and expecting a final automatic Unavailable outcome
instead of the established Running/Retrying state with a retained terminal candidate failure.
The barrier-bearing test path now uses an owned bounded worker and joins it before inspecting
channel success or failure. The terminal consumer checks the original typed refusal, actual
retry notice, withheld editor publication and cancellation of that same recovery session.

Run `671d0d62-a1b9-4414-8a28-4f67277b59ca` passed all 54 selected cases: 24 picker,
18 shell, six setup-service and six accepted first-conversation recovery cases. The canonical
checkout's 59 frozen source inputs matched the working source by SHA256. Independent review
found no remaining implementation defect in these corrected paths. Raw final evidence is
`.tmp/runtime-setup-mount-evidence/final-focused-tests.log`. This is a qualified mounting
checkpoint. Locked offline all-target compilation of beryl-app and Beryl with test-faults passed
in 1m 26s; raw evidence is `.tmp/runtime-setup-mount-evidence/final-all-targets.log`. The exact
temporary canonical checkout was reclaimed after process, absolute-path and reparse checks.
At that checkpoint the scope-memory policy remained unresolved and the phase was not accepted.

## Approved History And Mounting Acceptance

The Operator approved the 16-scope limit after its picker meaning and eviction behavior were
explained. Three added GPUI cases qualify the current-inclusive bound across 80 distinct scopes,
recency refresh, same-key revisions, evicted re-entry, selection eligibility and instance disposal.
Run `aff4734b-c8a0-42b8-ac97-2f6188ede536` passed the complete 57-case boundary; final locked
app/Beryl all-target checks and independent source, authority and evidence review passed.
See [accepted mounting qualification](../audits/runtime-root-setup-mount-qualification.md).
