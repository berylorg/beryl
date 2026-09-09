# Composer Marker Admission

## Invalidated App-Only Assumption

GUI marker insertion originally reached draft staging without marker writer admission. This
predated the recovery-view switching change and could not be resolved by waiting for more frames.

The composer slot dispatched through ordinary admission, while marker readiness was available only
with `test-faults`. Translation emitted an insertion marker effect, which normal staging rejected
when writer admission was absent
(`syndic-storage/src/draft_piece/staging/prepare_begin_page.rs`).

Three mounted tests reported invalid staging requests. Suppressing their owner errors, changing
expected marker counts, or extending fixed frame loops would not correct that authority gap.

## Suggested Correction

Connect marker-affecting GUI edits to the existing exact marker-readiness lifecycle before
`MutationBegin`, then consume its proof through marker-aware admission. Preserve selection and
operation identity, bounded preparation, cancellation, failure and move-only custody. The
[existing proof-composition boundary](syndic-draft-marker-cross-domain-proof-composition.md) and
[app contract](../../crates/beryl-app/doc/design-catalog-and-composer.md) already define the required
mechanism. Do not bypass storage validation, fabricate readiness in the app, or add a new policy.

The Operator authorized the missing prerequisite mechanisms. Bounded widget evidence/replay,
fresh Asset metadata witnesses, mixed Syndic label assignment, and read-only admitted target
resolution are now implemented and independently reviewed in the local checkouts. These correct
the earlier assumption that app-only wiring could authenticate fresh images and inspect complete
edit evidence before storage begin.

## Typed Refusal And Custody Boundary

App integration review exposed another reason app-only wiring cannot satisfy the existing
contract: public Syndic readiness and assignment APIs erased the required distinction between an
isolated operation exceeding its profile, aggregate temporary capacity saturation, and storage
failure. The source, page-submission and assignment APIs exposed no `OperationTooLarge` or
`CapacityUnavailable` result, and publication submission reduced all refusal causes to `Rejected`.

The app contract requires preserving those typed outcomes; the app cannot reconstruct them from
generic rejection without inspecting private state or inventing policy. Implementation stopped
before app edits under the Operator's technical-plan rule. The authorized correction now preserves
typed refusal causes through Syndic preparation, submission, assignment and reconciliation.
Public-boundary tests verify isolated versus aggregate limits, unchanged prior authority, and exact
cleanup and ambiguous custody. Do not weaken app outcomes or replace source classification with
app-side guesses.

The correction must preserve custody as well as error variants. Actual storage-fault tests showed
that a failed HomeStore health check during local cleanup could overwrite the original failure,
and that a committed command could lose its receipt and later failure through an unavailable
local-finalization result. Preserve those facts without claiming cleanup success or recreating a
retired capability. Independent review also found that retaining a postcommit readiness retry
while releasing its assignment attempt allowed the retry to observe a later assignment and issue
a second proof. Keep the retry's exact attempt exclusive and bind proof issuance to its selected
command. Verify no-mutation retry, competing-attempt rejection, drop-to-cleanup, and real journal
failure reconciliation through the public API.

## Staged Build Settlement Boundary

Production composition exposed another unsupported handoff. The app's
`composer_host/mutation/drive.rs::run_build_command` reads terminal operation status and discards
the HomeStore command outcome's local-finalization capability. That status read does not resolve
writer progress or release and reclaim settled writer authority.

Syndic's public `draft_piece/read.rs::reconcile_draft_piece_command_outcome` performs those actions
but unconditionally requests original fragment pages from ordinal one. The bounded app owns
durable staged authority, not a full fragment inventory. `draft_piece_operation_status_page`
requires actual fragments while fragments remain; an empty callback cannot satisfy it. The
durable staging window exposes fragments only under `test-faults`, and no production authenticated
fragment reader or endpoint-based reconciliation boundary was found.

The proposed correction is a bounded Syndic outcome reconciliation API authenticated by durable
staging/build authority and its exact endpoint. It must consume local finalization, resolve writer
progress, release successful settlement, and establish noncommit cleanup while retaining unresolved
custody. Do not substitute status reads, fabricate fragments, buffer the full edit, or add unrelated
global cleanup to compensate. Independent review and root inspection confirmed the public gap;
implementation stopped under the Operator's technical-plan rule.

The Operator subsequently authorized that API. The required long-operation test exposed the
[persistent sequence split-height defect](syndic-draft-piece-split-height.md), which was corrected
before outcome acceptance. The accepted outcome boundary now captures actual serialized results,
including dynamically selected history-capacity refusal and exact settlement replay, and retains
finalization, reconciliation and cleanup custody behind one opaque flight. It authenticates selected
V5 pending descriptors and current active fragments, the scanned endpoint and immediate predecessor
chain using one charged reader. A scanner's prior fragment cannot authenticate an active marker
transition. Cleanup remains separate from the committed transaction and each resume is bounded.

Independent semantic/adversarial review and isolated locked production Syndic/app checks passed.
Run `ce0a333c-13e1-45b7-a104-73b22e08d0f9` passed all 18 new outcome cases, all 15 writer-admission
cases and 23 of 26 history cases. The outcome cases include ambiguous reconciliation beyond 256
fragments with physical read accounting, dynamic settlement, pending-reference corruption,
byte-equal split publication, replay, local finalization and cleanup ambiguity. The remaining three
history fixtures also failed on pristine `8ff7bd6` in run
`8e79502d-aa93-4758-80e2-50721b536e4c`: they recovered HomeStore before consuming the original
generation-bound finalization capability. They now consume the complete original outcome through
the package helper before recovery and authenticate the persisted settlement afterward, preserving
all root/history, retention-floor, byte-budget and absent-successor assertions. The corrected three
passed in run `51fac404-11a4-496d-be8c-1f17952b4b83`. Do not move an unconsumed capability across
recovery or manufacture a replacement outcome to make a fixture pass. App integration now uses
the accepted outcome boundary.

## Evidence And Status

The typed-refusal prerequisite is accepted in `a50186a`, the mapped build frontier in
`0042f2e`, authenticated terminal replay cleanup in `35ebfdf`, and bounded settlement preparation
in `27aef10`. The last correction resolves the ordinary-stack postpromotion abort documented in
[the stack-layout record](syndic-composer-mutation-stack-layout.md#postpromotion-settlement-recurrence).
Moving fixture setup alone had not corrected the nested production settlement stack.

The app composes bounded widget evidence and replay, fresh Asset metadata, Syndic readiness and
label assignment, and exact target resolution before production mutation begin. Existing marker,
candidate/root/history, cancellation, generation and 256-byte surface assertions remain intact.
Current, detached and disposing hosts retain opaque outcome flights through finalization,
reconciliation and cleanup; terminal unavailable admission cannot be revived by retry.

Mounted review corrected a dequeued staging request being treated as an owner error when its host
returned `MutationWorkPending`. The owner retains the bounded exact request and shared cancellation
state, resumes it first, suppresses late acknowledgement after actual widget cancellation, and
drains the same operation. Authenticated noncommit may advance session generation while preserving
candidate, root, history and logical range authority; detached settlement cannot replace a live
binding.

Final review found two additional lost-error boundaries. Stringifying post-begin host failures
prevented Notifications from distinguishing committed edits, committed intermediate work and
unavailable outcomes. A successor-proof read failure after commit was also discarded because the
slot had advanced while the mounted editor retained its predecessor. Host errors now preserve
classification from the retained authenticated transaction result. Dispatch retains typed errors,
exact owner and mutation identity; committed presentation failure additionally retains the exact
successor identity. The original editor remains context, further dispatch stops, and the existing
Notifications contribution supplies a persistent explanation. Neither an intermediate command's
commit nor a presentation failure manufactures successful widget adoption.

Size, temporary capacity and storage refusals retain distinct dismissible explanations. Dismissed
records are not recreated by rerendering, stale dismissal cannot remove a later operation's record,
and unavailable feedback remains persistent without retry commands. Published owner and mount
identity authenticate presentation without taking the service selection lock during rendering.

The final test evidence covers 156 distinct focused cases on ordinary stacks with the accepted
LLVM, one-job, no-normal-debug and nonincremental configuration. Run
`e94a5f38-bfe2-4b3f-ad86-a9cf349adf79` passed 127 cases in 227.732s across marker evidence, mounted
composer, slot, notices, pending activation, host, history, mutations and publication. After the
successor-proof correction, run `3c7e1614-fcd8-42f6-a2dc-d1d766f3feec` passed all 69 lifecycle,
notice, mounted and pending cases in 142.301s. These runs overlap; they are not 196 distinct tests.
Their guarded peak commitment was below 1.5 GiB, and both jobs reaped all children.

Real host recovery witnesses preserve committed-versus-intermediate classification and exact
custody across repeated unavailable calls. Mounted transport witnesses retain actual post-begin
custody and inject the typed dispatch failure; the mounted shared HomeStore cannot execute the
consuming recovery operation. A separate mounted test injects a real storage read failure after
commit, verifies the persisted marker and released storage custody, and checks exact persistent
feedback without replay. Notice-owner retirement is exercised; replacement-composer rejection
and the dormant detached-flight branch are reviewed structurally. Tests named detached do not
claim runtime coverage of that branch.

Independent final review found no remaining blocking issue in these boundaries. The isolated
30-file production composition and five accepted storage corrections passed locked local metadata
and `cargo +stable --config .cargo/local.toml check --locked -p beryl-app --lib --no-default-features`
in 185 seconds. Every isolated app source matched the main checkout by SHA-256. Guarded peak
commitment was 3.30 GiB with no remaining children. Local app acceptance is complete; canonical
dependency publication remains the separate gate below.

Historical marker setup now admits a real published Asset through evidence and label assignment;
synthetic reused-label setup did not establish that authority. The malformed two-empty-page stream
is tested as invalid input, with retained custody, authenticated cancellation, unchanged logical
authority and a subsequent successful edit. No production filtering or storage-rule exception was
introduced. Mounted clipboard image decoding and large/rich paste remain a separate checkpoint.

## Canonical Dependency Publication

With Operator authorization, text-input `5f2f272b71666b56f990a1d2ee15e83219a05c9e` and
settings-window `8a9e1cd83eb8b837c1f9cdb51aad80a8a1446bd8` were published to their origin/main
branches. Beryl's formal pins and canonical lockfile now select those revisions. Updating only
Beryl's text-input pin left settings-window on the old widget and resolved two versions; the
dependent pin update preserves one canonical widget graph. No widget source changed during
publication, and the canonical lockfile changes only those two Git sources.

The development checkout automatically includes `.cargo/local.toml`; plain Cargo commands there
still use local patches and the ignored lockfile. The earlier ENV.md statement that plain commands
were canonical was incorrect. Canonical resolution and validation used isolated checkouts with
tracked Cargo configuration and no ignored local configuration. Only their verified canonical
lockfiles were transferred back. The Beryl ignored local lockfile retained its prior hash.

Locked canonical metadata verified one GPUI, one text-input and one settings-window package from
their published Git sources. The settings library check passed, followed by the isolated locked
production app check without default features in 172 seconds. The latter peaked at 3.31 GiB of
guarded commitment and reaped all children. Scoped completion review passed; semantic navigation
restarted only after successful validation, and a subsequent symbol query passed. The publication
boundary is accepted independently of deferred process-provider composition.
