# Large-Draft Clipboard Refusal Qualification

Status: accepted after independent consequential review, 2026-10-06.
Evidence for root plan phase 731.

## Controlling Boundary

The [composer clipboard semantics](../../features/composer/design.md#image-clipboard-semantics),
[private source ownership](../../systems/image-assets/design.md#private-composer-clipboard-sources)
and [app clipboard operation ownership](../../../crates/beryl-app/doc/design-catalog-and-composer.md#clipboard-operation-ownership)
require a successful cut and a later refused paste to remain separate operations. Refusal preserves
the cut successor, directed selection and cut history. Healthy determinate refusal preserves source
eligibility; an adopted origin undo or edit expires it. Physical storage failure preserves exact
presentation and outcome custody subject to the ordinary failed-generation and recovery gates.

## Qualification Scope

Use the actual selected mounted composer, a 3 MiB paged draft with nonresident marker content,
injected checked clipboard readers/writers, and bounded rendering and operation resources.
No Operator clipboard access or native application harness is permitted.

The fixture uses a 64 KiB contiguous clipboard allowance and 4 KiB mutation pages. It inserts
markers at byte zero and EOF, verifies the first is outside the realized object set while the
selected tail marker is resident, and actually cuts the tail through the acknowledged checked
writer. The complete fallback derives from that selected marker's authenticated label.

Later private paste uses the captured immutable checked item. The size case sets the host's
test-local association allowance to zero; the capacity case sets its retained-head allowance
to zero. These exercise distinct real admission predicates without changing the production V1
profile. Each repeats four times, compares exact root/history/directed surface and complete
private descriptor, and checks distinct dismissible Notice identities and bounded custody.
Undo then restores the exact predecessor and selection and expires the source; a smaller ordinary
text edit proves continued progress. The storage case uses the real `BeforeCommit` physical fault.

Complete bounded durable marker facts include both the origin and EOF anchors. They preserve
marker IDs, order, labels and assets across cut/refusal and healthy Undo. Realization high-water
counters remain within their configured bounds; retained pages never materialize the whole draft.
Normal disposal retains an observational source-owner clone until the token no longer resolves
and weak composer, input and service handles no longer upgrade. Window disposal expires the
origin source; complete process-owner retirement remains a separate accepted graph lifecycle.

The [direct-marker evidence](qualification.md), [copy/cut evidence](private-copy-cut.md) and
[captured-paste evidence](captured-paste.md) remain accepted within their unchanged scope.
They do not by themselves qualify this combined large-draft clipboard workflow.

Healthy size/capacity refusals compare the complete private descriptor before and after refusal
and dismissal. Physical storage failure closes the old generation's command/read gates. The
ordinary source descriptor accessor validates healthy live authority and expires an invalid source;
it is not a cached-state inspection seam. Storage qualification therefore compares the retained
cut root, history and coherent widget presentation without claiming usable failed-generation
clipboard eligibility or Undo. Ordinary recovery replaces that generation and expires its source.

Canonical qualification uses the committed dependency graph without ignored local Cargo overrides,
locked stable Cargo, one build job, LLVM linking, no normal debug information, nonincremental
compilation and serial nextest. Bounded machine-local logs and resource ownership are recorded in
ignored `ENV.md` and `.tmp/large-draft-clipboard-evidence`.

The [qualification correction record](../../failures/large-draft-clipboard-qualification.md)
preserves invalid initial assumptions about canonical working directories, selected labels,
nonresident-marker preconditions, failed-generation inspection and process-owner retirement.

## Completion Evidence

The final three mounted clipboard cases passed 3/3 in run
`459759d1-1e90-46b5-941b-43ec62faf8cf`: actual cut followed by size refusal, capacity refusal or
determinate storage refusal. The healthy cases each include four refusals, exact Undo, smaller
editing and source expiry; all three include normal disposal and weak resource release.

Seven unchanged affected regressions passed in run `7e5d4141-1422-4574-9905-211d978f4526`:
three clipboard Notice cases and four large direct-marker cases. That run's three new cases failed
at the invalid initial label assertion and are excluded from acceptance. The interrupted footer
artifact is also excluded. Ten distinct cases are qualified across the accepted results; this is
not a claim that either earlier failed run passed in full.

All five recorded test/manifest inputs match the canonical checkout by SHA-256, independently
recomputed. The canonical baseline is `4c190f391f2ab323ab93b52415c76db4bf1d1219`; production source
and dependency pins are unchanged. Final locked app/executable all-target check passed in 1.74s.
Exact-path Rust formatting and scoped whitespace checks pass.

Independent consequential completion review accepted the complete large-draft clipboard boundary,
with no remaining blocking findings. No Operator clipboard access or native
application harness occurred. Qualification logs remain bounded below 8 MiB; the exact canonical
checkout was reclaimed after review and the shared target directory is retained. No owned build
or test process remains, and process-local stack settings were restored.
