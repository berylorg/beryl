# Native Transcript Interaction Evidence

Historical replacement qualification on 2026-10-13 exposed a recurring fixture assumption:
retained transcript state was treated as proof that a native interaction was ready. That assumption
produced false failures for geometry, subscriber delivery and menu activation, and initially hid
the real resource-provenance defect. It did not require changing product behavior.

Virtualized rows may retain measured geometry outside the viewport. Native fixtures must scroll
the actual outer surface, acquire current geometry again, and verify that the content hit point
is inside the viewport. Bypassing clipping or reordering production hit testing to admit an
offscreen point is invalid. Likewise, recovery assertions must inspect the canonical authored
child and narrative source rather than require a legacy text representation.

GPUI queues `Context::emit` effects and flushes them when the outer `App::update` finishes.
An action and its subscriber-delivery assertion inside that same update cannot prove delivery.
Separate the native action from the subsequent observation, including preview dismissal and
return-focus checks. Anchored menus also retain their actual focused row: disabled Copy remains
focused initially, and Enter correctly does nothing. Navigate to the enabled Edit row before
asserting its activation instead of changing menu semantics to satisfy the fixture.

Current geometry and a callback still do not prove command authority. Code menu admission
correctly rejected a provider record whose authenticated resource payload had not been copied
into its source provenance. The correction carries that same resource identity into both
projection and view records while retaining every existing ownership and currentness check.

Captured branch qualification additionally showed that narrative selection is not the complete
assistant-text surface. Code selections use canonical source coordinates already supplied by
indexed Source pages and their binding. The provider subtracts the binding start only for its
resource-relative read, then returns canonical coordinates. Adding a fence or binding offset again
would target different bytes. Native Code fixtures should use current measured source endpoints
and preserve Code-owned selection rather than reconstruct narrative selection or exported Copy
text. A retained Retry must save its currently invoking origin before staging can clear that
selection; a backup left by a different ended capture is not evidence for the retried request.

A small fully realized Code resource may retain only an indexed Display response. Its authenticated
`Fragment.text` is raw UTF-8 read from the exact `Fragment.source` interval, not a formatted export.
Matching binding, root, metadata and current rendered coverage can qualify those retained bytes
without a second Source response or offscreen read. An empty placeholder fragment proves no
nonempty selection. Requiring a separate Source page would disable a legitimate rendered capture.
Host resource pages can also be released after layout preparation, even while Code stays ready.
Capture must follow the layout owner's bounded exact-byte custody through that transition;
ready geometry alone cannot authenticate bytes, and transformed renderer text cannot replace them.
Code layout's authenticated Range seed presentations supply the original layout bytes and are
already charged. Seed reuse alone proved insufficient: range reconciliation can release them
while prepared Code remains ready, and later Display/Source-only deliveries are not seed-owned.
Exact-byte custody must follow the currently prepared source ranges, with release and charging
inside the existing realization budget. Prepared geometry and streaming text placeholders cannot
substitute for that source proof.

Indexed resource demand owners legitimately retain their creation presentation revision while
resource publications advance the shell revision. Existing owner currentness authenticates exact
activation, view/provider revision, protected record, source provenance, resource identity and kind.
Comparing that older demand stamp with the current shell presentation disabled valid Code captures.
Separate logical source-owner authentication from the fresh invocation stamp; retain exact current
menu/command and measured-layout fences. Contiguous multiline selection also needs authored LF or
CRLF coverage between rendered content lines. Use the authenticated CodeLine terminator contract,
never infer separators from formatted Copy output or bridge an oversize placeholder.

A first measured Code endpoint does not prove that the complete current viewport has finished
layout. Wait for current ready layout before a native menu invocation; a later ready frame does
not retroactively execute an earlier rejected key event. Likewise, same-view source refresh builds
a fresh core and reestablishes indexed demand through ordinary resource reconciliation. Preserved
selection alone does not prove that a checked Copy has current read custody. Wait for the current
indexed owner and settled demand before the first native Copy, retaining the real retired-source
refusal for an action whose source changes after admission. Indexed owner currentness alone is
insufficient: the checked provider read also authenticates the published head and history summary.
After failed activation, wait for ordinary publication to match the actual current parent head and
summary before invoking Copy once. Do not retry a refused native command or bypass its reader.
Likewise, a later horizontal Code menu invoked while resource staging remains in flight is correctly
invalidated by that delivery. Wait for staging quiescence before its first invocation rather than
preserving a menu across source publication.

Final diagnostic cleanup changed a fixture facts string from an optional debug value to a boolean.
An assertion still matching the old string failed before issuing the native right-click. Behavioral
readiness assertions should use typed predicates; diagnostic formatting is not an acceptance API.
An idle resource worker also does not establish quiescence when a demand has not launched or a
completed delivery awaits measured publication. Observe queued demand and staged publication
together with current source and geometry before the first native invocation.

Authenticated CodeLine metadata alone also cannot grant rendered raw-line coverage. A native drag
from an oversized combining-character placeholder's end to the next line's start can select only
its LF. Both lines may be prepared, but the placeholder retains no raw source bytes. Separator
bridging requires both adjacent prepared lines to be blank or retain a genuine raw fragment;
full selected content still needs contiguous raw-bearing coverage. Preserve positive multiline
LF/CRLF and Tab behavior while refusing that placeholder-only boundary.

Diagnostic activation fixtures must hold a resource read that the ordinary initial viewport
actually demands. A long Code line earlier in the history does not prove a pending read while
activation places the viewport at the narrative tail. Range or Display responses can also already
supply its bounded raw content without a Source request. Qualify the real requested resource kind
and current revision in the visible target, preserving normal demand and publication; do not
inject offscreen demand or add an activation gate to manufacture pending evidence.

Tail-oriented activation uses the explicit LiveTailFollowing mode. Its active semantic target is
the measured last record at the viewport bottom; the first-visible reading representative can
change when preceding row heights refine. Compare the actual published tail identity/bottom,
published viewport height, mode and manual displacement through resource settlement. Do not impose
detached reading-anchor invariance on Tail mode. Short-content leading alignment retains its
separate first-anchor proof. Neither case permits a new late placement command or readiness gate.

Diagnostic view observation must use coherent selection custody rather than creation eligibility.
The creation-target getter intentionally returns unavailable during pending activation. Observe
the existing live composer/claim agreement directly rather than equating command availability with
selection. Ordinary flush can also legitimately park the predecessor composer in Fencing while
the activation worker owns its original typed custody. Report the live selection unavailable at
that cut, alongside the unchanged published view and exact pending target; do not invent a claim
from frame identity, duplicate locked custody or weaken the lifecycle gate. Likewise, generic count/query error state
cannot stand in for an activation outcome: healthy polling clears it and query failures reuse it.
Retain the bounded exact-target activation failure at its actual lifecycle boundary, independently
of unrelated polling.

Cancellation ends at writer admission. A token cancelled while a `BeforeCommit` fixture hold is
already inside the active writer cannot require noncommit or predecessor preservation. Use a
genuinely missing exact target to qualify retained activation failure, and release an admitted
held activation to its ordinary exact outcome. EOF evidence must observe the actual stdio terminal
and attach's Exit handoff, then positive single-operation settlement and Home release. A transient
startup-command pending flag may already be consumed by the running owner; it is not a durable
receipt that EOF was or was not handled.

An EOF Exit handoff while thread activation gates Exit is not accepted shutdown admission.
The ordinary command correctly consumes that unavailable invocation without queuing a later Exit:
the exact activation can commit and settle, leaving the same window alive with Exit eligible again.
Observe the terminal endpoint, single handoff, single original activation outcome and absent
shutdown attempt. Qualify disposal with one fresh explicit ordinary Exit after eligibility returns;
do not add an automatic retry or claim that the gated EOF invocation itself released the Home.

Native Code recovery/range routing passed in `run-046`; the complete grouped image workflow
passed in `run-050`, including Code/Table/marker origins, cross-marker selection, primary pointer
and keyboard actions, preview dismissal, focus return, actual Edit/import, dimming and Escape
cancellation. These lessons inform the remaining transcript workflows in [the plan](../plan.md)
and the [transcript contract](../features/conversation-threads/design.md); they are verification
guidance, not a replacement for their authority.
