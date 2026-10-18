# Native Main-Window Minimum Qualification

Beryl integration acceptance passes for the declared Windows scope. This record separates
dependency evidence from application publication and containment qualification under
[the plan](../plan.md) and [shell lifecycle](../../crates/beryl-app/doc/design-shell-lifecycle.md#main-window-minimum-publication).
Other GUI platforms remain unsupported until their native backends implement and qualify the
same guarantees. Broader restoration/progressive-readiness acceptance remains separate.

## Native Dependency

The runtime capability is accepted at GPUI `4529c9305c53a921871b9c84e388321d7a8daa9d`.
Locked metadata and the production library check pass. Independent source review accepts the
original-owner, native-state and acknowledgement boundary. The configuration-correct final
replay passes all 29 model/native/lifecycle cases in 10.780 seconds
(`b227832a-14c0-4808-94c1-5892b76091c1`). It uses `cargo nextest run --config` so the child
Cargo build receives the LLVM override. Earlier packets remain historical behavioral evidence;
they did not forward that override. No source or lock changed for this replay.

The [fork audit](../../../zed-fork/doc/audits/window-minimum.md) contains native scope and limits;
its replay correction is pushed in audit-only commit `f1bb630dd69e2da0ea5d5378ee82b60ed3525bce`.
Widget pins retain the qualified runtime revision. Raw final receipt and configuration are retained
under `.aipm/tasks/native-main-window-minimum/run-005` through integration acceptance.

## Canonical Widget Alignment

Scrollbar `522555c8ade88f5798b271b4ec056d142810ae0b` is committed and pushed. Its isolated
canonical checkout has one GPUI package at the qualified Git revision. Locked metadata and the
production library check pass (10.67 seconds); all 63 cases across eight explicit integration
targets pass in 0.522 seconds (`e2474654-3da5-43fe-bedb-aeadf328f68a`). The manifest and lock
change only the GPUI revision and its fork-owned package source identities. The checkout and
test temporary directory are reclaimed; inputs, graph metadata and logs remain in the task scope.

Input `7e80c3e187b2493215af40b4dd60cdd9977f37f8` is committed and pushed. Its canonical graph
contains one GPUI and the accepted scrollbar revision. Locked metadata and the production library
check pass (10.38 seconds); all 607 cases across the library and 11 explicit integration targets
pass in 46.391 seconds (`7eb1209e-e1b5-46a4-a22a-5686278cd8af`). Its lock changes only the
GPUI and scrollbar source identities. The accepted checkout and temporary directory are reclaimed.

Settings `2c6e277c21eaacec04c7dcbf4779d333cee43d94` is committed and pushed. Its canonical
graph contains one GPUI and the accepted scrollbar/input revisions. Locked metadata and production
library checking pass (4.28 seconds); all 109 cases across seven explicit integration targets pass
in 3.417 seconds (`fc32a2e7-3b8c-472f-8b27-fed2236e9292`). Its lock changes only those source
identities. The accepted checkout and temporary directory are reclaimed.

The final graph additionally supplies shared checked layout before construction and the bounded
test-frame query, qualified in [the fork audit](../../../zed-fork/doc/audits/preconstruction-layout-and-frame-bounds.md).
GPUI is pinned at `ac74530ef933b3239cdc0ad03641c466323ab35b`, scrollbar at
`2fa5fc92132d171a7b572ef19121ffb7d684a654`, input at
`ec0e2af5dc05da286312f742af345f5a0cbda28c` and Settings at
`2aff8aa3be7fa26e865ad8de5653549d93130840`. All are committed/pushed. Each final widget
alignment passes isolated locked metadata, one-GPUI graph validation and its production check
(9.86, 9.90 and 4.06 seconds respectively). Widget source is unchanged; prior full widget tests
are reused because checked layout retains the identical implementation and window access,
with parity proved by the 13-case font-environment packet. The new bounds query is test-only.

Beryl's canonical graph resolves these four exact Git revisions with one GPUI package. The normal
App/executable check passes in 28.34 seconds and the fault-enabled App check in 46.59 seconds.
The local development model also passes locked metadata and its App check (28.65 seconds), then
the language server restarts successfully. The canonical tracked lock SHA256 is
`28F44872F4B75A6643FFBE2A685B515389C0FCECA4A3A8D0F78A9A75F355D454`.

## Application Review

Initial source review found one bounded ownership defect: an unchanged-geometry appearance
commit could discard a pending minimum-relaxation flight, whose detached completion could later
clear a different slot. The correction preserves pending ownership and matches completion to the
original flight. The deterministic interleave regression, independent correction review and
production compilation pass. No editor/source loss or unsafe partial
appearance publication was established by that finding.

Operator selected Windows as the currently supported platform. Other backends reject publication
until they implement and qualify the same shared contract. The application adds no platform bypass.

The native tests now cover default restored/threadless and 80-pixel editor metrics through the
ordinary restore-set producer, plus visible lineage through actual Application startup, mounted
RunningProcessOwner and ordinary picker activation. Independent source review confirms the
required slices and existing custody, identifying one Exit eligibility/invoke split in the new
fixture; its bounded correction checks eligibility and invokes once in the same GUI update.
Run 008 completes 64 shell/appearance/recovery cases with 61 passes in 143.097 seconds
(`0b26cc09-955c-40de-bcbd-cbac7ab1e7c4`). The original-flight/color-only interleave regression
passes. Two fixture assumptions fail: a fractional simulated client minimum is not a whole
physical pixel, and an 800-by-600 prepared window is smaller than its new derived minimum.
The owner-release fixture also fails to flush the GUI release effect. Corrections retain strict
containment, exact adequate prepared geometry and genuine owner release. A separate explicit
retirement test covers cancellation of an active native-readiness wait with its owner still live;
the source correction reuses the existing mailbox waker. Run 010 qualifies these corrections.

Run 009 executes two genuine native cases in 8.276 seconds
(`9a8aec36-7e0e-4aee-831a-c1abef97f75e`). The restored/threadless/default/80-pixel editor case
passes with actual client geometry, native tracking floors and rendered selector containment.
The lineage case aborts before its scenario in the inherited title-based HWND lookup. The
corrected helper uses GPUI's existing read-only native-handle interface, without retries or title
mutation. That invocation establishes no lineage geometry outcome. The test-only dependency adds one
existing resolved dependency to App's lock entry; it introduces no new package or GPUI API.

Run 010 completes 67 cases with 66 passes in 136.054 seconds
(`4aaa47a2-17f5-4044-81be-07585629efb8`). All three original run-008 failures pass after their
reviewed corrections, as does explicit retirement while native readiness remains held. The
genuine native restored/threadless/80-pixel case passes again. The read-only original HWND
query succeeds; the lineage fixture then fails its following placement capture. Startup completion
already follows whole-set publication. Subsequent creation-control attachment changes the visible
minimum, and capture also refuses during an active native operation. The generic error does not
directly identify the failing native predicate; source review establishes a missing post-attachment
readiness cut. The correction observes original-shell readiness and native-flight settlement,
then captures once.
The failed invocation still establishes no lineage geometry result; retain its original evidence.
The new tracked lock is `E3D7655490382EB645A2AB6D77FE484CF256B08819A5DBF8459F939D266CD8FE`;
canonical and local locked metadata/App checking pass (15.95 and 16.53 seconds), and the analyzer
restart succeeds. This is correction-boundary evidence, not whole application acceptance.

## Accepted Application Boundary

Run 011 passes all 60 cases across `main_window_restore_set`, `main_window_composer`,
`main_window_composer_mount`, `main_window_composer_slot` and `native_close_admission` in
122.761 seconds (`bb6e9a7b-ebed-4d68-8f06-3bddf740532c`). The final shared-fixture correction
preserves the original publication, geometry, generation and contribution checks, waits for
actual native-flight settlement through bounded original-root observers, and captures once.
Its existing absolute deadline and whole-fixture watchdog remain unchanged. Independent source
review passes without a lost-wake, ownership or guard-relaxation finding.

Run 012 passes the genuine visible-lineage native minimum case in 4.190 seconds
(`c4900e61-b1f6-43a4-af56-a3253433c043`). Its final source patch is
`6EEA6A83F878B8BF4AB6EA693276F8D9D81CCEADB0D6749AC966997ED8173AEC` and 44-file source
manifest is `F970416577D6B9B95AE46607C1A21EA74C3CAD3BB9802DCF424BCF2CD20C802C`.
Relative to run 010, only the already-reviewed shared test helper changes; production source
and every run-011 integration target remain unchanged. The union contains 126 distinct passing
cases: 66 from run 010, 60 from run 011, one overlapping native minimum case, and one final
lineage case from run 012. Failed earlier invocations remain recorded without becoming passes.

Independent whole-boundary review accepts the Windows implementation: native containment for
default, threadless, changed-editor and lineage composition; original focus/window/editor/source
identity; committed/candidate minimum protection; exact-flight ownership and relaxation;
atomic appearance publication, hidden registration, refusal, retirement and release; and the
affected lifecycle consumers. This does not accept other GUI platforms or an untested Windows,
DPI or monitor matrix. Current native evidence uses Windows 11 Pro build 26200.

The integrated restoration/progressive-readiness boundary has completed its required refusal
proofs, large-case qualification and936-case App union; see the
[runtime qualification](restoration-bootstrap-runtime-gap.md). Root commit/push includes both
accepted boundaries; accepted dependency commits are already pushed. Historical failures remain
recorded with their evidence limits. Completed run-005 through run-012 temporary
directories, including the two failed native fixture homes, are reclaimed after final review.
Their source patches, inputs and logs remain available; shared build artifacts are untouched.
