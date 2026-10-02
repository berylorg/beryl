# Automatic Interrupted Exit Recovery

## Pending Draft Release Before Publication

Phase 702's initial automatic supervisor reused the explicitly driven publication pass as a
one-shot operation. Independent lifecycle review identified that
`publish_interrupted_exit_services` converts an unfinished recovered draft release into an error.
The selected service's nonblocking lock may legitimately return busy, and recovered autosave
preparation may also remain pending. The automatic task then retained an unavailable outcome;
same-request coalescing prevented another failure notice from driving that pending work.

The [app lifecycle contract](../../crates/beryl-app/doc/design-shell-lifecycle.md#interrupted-exit-recovery-ownership)
requires pending release to preserve completed ticket evidence for a later pass. Automatic
composition must drive that pass before dispatching publication, rather than treating temporary
contention as terminal failure or repeating publication after an uncertain result.

The correction polls exact recovered draft release with cancellation and a bounded timer before
the single publication dispatch. Per-mount completed release evidence remains reusable; no GUI
callback separates the ready pass from dispatch. Source review confirms the corrected ordering.
The automatic native deferral case passed within thirteen-case run
`2eddf259-3ba3-4344-bf14-ab12a85b665a` and the broader verification below.

## Cancellation After Candidate Preparation

The same initial supervisor treated a driver cancellation as sufficient terminal settlement.
Review identified a supported cancellation point during prepublication draft polling: the prepared
graph remains in `CandidateSettlement::Services(Ok(...))`, and successfully adopted residents
still hold fresh service resources. Returning `Cancelled` preserves interaction fencing but does
not fence, join and abort that candidate as required by the
[same-home recovery contract](../systems/backend-runtime/design.md#same-home-recovery-composition).

Automatic cancellation needs cleanup appropriate to its retained preparation and attachment stage,
preserving actual original and resume outcomes; closed gates alone cannot establish disposal.
An adopted fresh mount is `Preparing`
without a flush ticket and has consumed its predecessor snapshot. The old resident retirement
path requires `Ready` with a flush ticket and rejects an active prepared graph. Reusing that path
unchanged cannot supply the missing cleanup. The correction supplies exact unpublished-resource
detach, resident-flight drain and joined graph abort before reporting settled cancellation.
It distinguishes actual successful publication from a failed publication result retaining a
candidate, and retains its exclusive driver token across cleanup waits.

The first fresh detach implementation exposed a further lifecycle gap in native run
`3c55b86e-2e25-42ab-8088-f28bf762c31b`: resident workers were empty, but the widget was not quiescent.
Fresh adoption released predecessor protection, allowing later paint to queue layout work while
the recovery-fenced resident could not pump it. Waiting for quiescence at cancellation therefore
could not finish. The correction retains exact fresh widget protection atomically at adoption
through unpublished recovery, making it available to cancellation detach. Fallible widget
enablement stays before aggregate process reopening; moving it after reopening could turn a
permitted capacity refusal into a panic.

Run `a2d0707f-ff97-44d3-abe2-ecd07dd0f73f` then showed protection invalidation during ordinary
recovery: the production configurator retained construction-time geometry although native paint
had updated the resident's viewport and wrapping. Fresh configuration must combine immutable
resource limits with the resident's current layout/style/viewport, captured through the widget's
bounded read-only observation boundary. Copying original construction geometry is not sufficient
proof of a matching recovery environment. The widget's read-only snapshot passed seven focused
tests in `9df7085a-497f-4250-92ec-5028f60ed363`, default compilation and independent review; its
accepted revision is `dcbe549227df367184a6867a80cce5c031b63188`.

Retained `Services(Err)` also requires exact home-return validation during cancellation. An
unconfirmed retirement cannot default to successful cleanup: return refusal preserves unavailable
custody. Native tests hold a real provider reference to exercise that refusal, repeated attempts
and subsequent fixture cleanup without synthetic retirement flags.

## Verification

The affected native aggregate `b67c3aeb-d14a-473b-9b6c-814ea196bd46` passed 150 of 175 cases.
Its 25 failures exposed stale geometry in older explicit fixtures and artificial disabled-input
focus. Fixtures now capture current geometry while retaining protection/refusal and identity
assertions. Focus setup uses the actual shell focus target; existing live-editor-before-shutdown
tests still prove focus preservation. The corrected affected run
`f31aa895-7204-4418-8636-58adbff8c8f9` passed 58 of 59 cases; its remaining fixed-geometry appearance
fixture passed after correction in `0234aa70-6be0-4b6c-991a-c4ad05fb7427`. Unaffected passing cases
were reused, not represented as a wholly successful first aggregate.

All 19 automatic cases and both real service-retirement cancellation cases passed. Coverage
includes original commit/noncommit/exact-new reconciliation, retry, known resume outcomes,
duplicate/stale requests, owner drop, pending draft/mount completion, partial multiwindow cleanup,
resident-flight joining, publication-worker cancellation and fenced postpublication cancellation.
Default and fault-enabled app checks, scoped formatting and independent lifecycle/persistence
review passed. Published widget `dcbe549227df367184a6867a80cce5c031b63188` and Settings
`6e233a385e48b3110f62933dc68c4774785443fb` retain one canonical GPUI/widget graph. Isolated locked
metadata and fault-enabled app/test compilation passed; local metadata/library validation and the
required analyzer refresh also passed. Independent review accepted the final pins and phase 702.
