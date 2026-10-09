# Running Thread Selection Publication

## Invalidated Assumptions

Mounted Running threads integration cannot reuse composer publication as one uninterrupted GUI
operation. Its source preparation, prior-draft flush, durable claim replacement, widget release
and coherent presentation have separate ownership boundaries.

The first integration assumed a known rejected claim could retire the pending target after the
prior flush through ordinary retirement. The canonical slot was already Publishing and correctly
refused that operation. It also assumed the shell's initial composer binding remained current;
ordinary edits advance the live binding without changing the selected claim.

A subsequent custody audit found that a consuming widget-settlement API could return only an error
after failing to acquire the slot or validate released requests, dropping those requests before
their settlement was proved. Holding the process and window-registry admission guards across the
durable claim command also made blocking GUI capacity reads wait for storage.

Mounted qualification additionally exposed a liveness assumption: returning during the source
cooldown did not schedule a feature-owned wake, and retaining the initial flush state ignored
later canonical `Progress` states requiring another capture. Frame activity and a read-drain timer
cannot be the activation owner's progress guarantee.

## Correction And Evidence

The coordinator retains admitted claim and widget custody outside disposable read tasks. A known
uncommitted claim uses an exact abort before old-widget release, after the predecessor's canonical
saved checkpoint is qualified. Shell lookup authenticates the live composer against the cached window and
claim, then uses its current binding. Source settlement returns owned work with pre-settlement
errors and retains the release proof after settlement. GUI admission facts use nonblocking reads
and show the existing busy state while the commit guard is held.
The operation owns one identity-fenced delayed wake and follows each canonical flush advance using
the original ticket, without beginning another flush or replaying a settled mutation.

Evidence is in the canonical claim-publication driver, the mounted
`running_threads_attachment` tests, and
`shell_capacity_and_selection_gates_return_while_claim_commit_is_held`.
Component checks alone did not establish mounted qualification or independent completion review;
the accepted boundary below records that later evidence.

The controlling contracts remain the conversation-threads feature, app catalog/composer and shell
lifecycle supplements, and transcript shell boundary. No running execution, CAS loading or
capture-consumer replacement is authorized by these corrections.

## Ordinary Flush Disposal Boundary

After the retry-wake and flush-state fixes, all three mounted Unviewed scenarios reached ordinary
ThreadSwitch flush disposal and failed. `SyndicComposerHost::advance_flush_disposal` clears
`active` and lifecycle state on successful disposal. Retiring the target after a rejected claim
then removes the slot's pending cached identity; the prior host has no live binding to resume.
The existing exact abort helper is sufficient only while that prior host remains live. Simply
calling ordinary disposal before the claim does not complete the activation correction above.

Operator approved the correction: the canonical composer must qualify a source-owned saved
checkpoint while retaining the prior host until the target claim is known committed, then complete
canonical disposal and widget release. The
[catalog/composer contract](../../crates/beryl-app/doc/design-catalog-and-composer.md#range-backed-composer-host)
owns this distinction; ordinary ThreadSwitch, close and submission retain their completion semantics.
The approved correction required implementation and qualification of the selected boundary; the
decision alone did not establish acceptance.

Service-memory retirement alone is not durable candidate-session disposal. Retain the original
flush barrier through the claim decision: exact rejection can release its save fence while the
prior host remains live; known commit must drive canonical clean-disposal capture and reconciliation
on that same attempt before releasing the widget. A disposal failure after commit retains cleanup
custody and the committed claim, rather than restoring the predecessor.

A matching save-capture refusal can already have settled the host barrier. Advancing that stale
barrier while leaving the slot in its saving state prevents target retirement and strands the
selection lease. The slot must record the exact known refusal and retire the target immediately;
an unrelated stale capture or indeterminate publication retains its operation custody. Mounted
cancellation during capture qualifies this transition separately from cancellation during claim
commit.

Run `ac541f29-990e-488c-b06a-a1c5613eb1d2` passed five affected virtual recovery tests and failed
the three mounted activation scenarios. Bounded diagnostics remain in
`.tmp/running-threads-source-evidence/mounted-activation-recovery.log`. That run established the
original disposal mismatch; the accepted correction below supersedes its blocker.

## Accepted Mounted Boundary

Accepted on 2026-10-04: every main window mounts the bounded Running threads command and picker,
with exact Current acknowledgement, existing-window reveal and Unviewed attachment. Source-owned
saved-checkpoint qualification retains the predecessor until known claim commitment, then drives
canonical durable disposal and widget release. Failed save or known rejected claim retires the
target; indeterminate work and committed cleanup failure retain their exact operation custody.
Ordinary composer close, submission and ThreadSwitch completion semantics remain intact.

The final mounted run `afe78f5b-085a-433d-ae7f-c0c8241e8ba5` passed all seven cases: edited-prior
success, cancelled and indeterminate save, cancelled and indeterminate claim, committed disposal
failure and a real admitted checked-out CAS session preserved through the actual row callback.
The fixture verifies exact session authority, bounded connection/request records, input gate,
execution binding, session inventory and worker pool, with one window and zero new turn starts.
Quiet router polling legitimately changes the aggregate work revision; live-session evidence
compares protected records and source generations instead of that poll counter.

The preceding combined run `c4762a64-80fb-419f-afa1-d34f4541fc8e` passed 38 ordinary lifecycle/slot,
six source save-proof, five shell and five recovery cases. Its four mounted fixture failures were
corrected and qualified in the final seven-case run; unchanged production inputs permit reuse of
the other 54 cases. Earlier qualification also passed 20 picker/shell, 11 provider/geometry, 99
affected transcript and 22 attention cases, plus focused source, claim, lease and owned-payload
checks. The isolated locked library check passed against the tracked GPUI dependency revisions
without local overrides. Bounded logs remain under `.tmp/running-threads-evidence` and
`.tmp/running-threads-source-evidence`.

Independent full lifecycle, persistence and external-effect review passed with no blocking
findings; scoped rereview covered the save correction, exact executable/title presentation and
final fault/custody fixtures. Mounted tests use the virtual GPUI context. No native Beryl instance,
CAS replacement turn, input replay, hidden window or additional capture consumer was created.
Formatting and whitespace checks passed. Known failed homes and the isolated checkout are gone,
live fixture server shutdown joined, and no task-owned build/test worker remains. Only bounded
evidence logs and shared Cargo artifacts are retained. Complete transcript and other toolbar
mounts remain separate rework boundaries.
## Admission From The Updating Native Root

Actual process-owned ordinary selection in run `deff79d6-fbec-4dc7-a665-f382daf82a01`
reentered the invoking `MainWindowShellRoot`: the attachment callback already owned its GPUI
update borrow, while process membership enumeration read that same root again. All ten native
fixtures aborted before their recovery scenarios. Virtual fixture leases did not exercise this
production admission path. The downstream native callback panic is not stack-exhaustion evidence.

Carry the current root's entity identity and authenticated controller window identity into exact
published-process membership validation. Require exactly one matching published root; use that
current identity only for that root and validate every other shell normally. Preserve Home,
native reservation and selection-lease guards; do not introduce a cached membership registry or
substitute a fixture lease. The bounded correction and actual native qualification remain open.
