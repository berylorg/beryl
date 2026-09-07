# Findings Register

Baseline: `e6172f7c49bebef78d51831e621e70c8ac0d6a07`.

The audit is complete. Read the [whole-codebase report](report.md) for final conclusions and use
[canonical estimates](estimates.tsv) for current amounts, dispositions and overlap resolution.
The preliminary and file-review sections below preserve historical evidence. Their maturity and
amounts describe those earlier review stages and are superseded wherever final synthesis differs.
Do not add their subtotals or preliminary leads to the canonical total. All reductions remain
unimplemented proposals; a completed review does not mean Operator approval.

# Preliminary Findings

## PRE-001: Remove Ineffective Persisted Piece-Removal Progress

`crates/syndic-storage/src/draft_piece/persistent.rs:3174` plans a marker-free removed range, then
the `Removing` branch advances a cursor without changing the tree. `Applying` performs the actual
split/join deletion. The mutation, authenticated transition, contribution, and receipt paths show
that removing N pieces adds N+1 durable advancement commands before applying the edit. Eliminate
this persisted phase and its validation/codec/hash handling. Estimated net reduction: 95-130 lines,
with a runtime benefit beyond source size. Requires an explicit V7 schema/design change. Verify
empty and large removed ranges, marker exclusion, interruption, restart, receipt authentication,
and unchanged final tree/history semantics.

## PRE-002: Share Composer Test Construction

`crates/beryl-app/tests/main_window_composer_mount.rs` and `pending_composer_activation.rs` contain
23 mount-construction sites repeating selected-host, slot, service, entity, and default widget
configuration. Introduce small constructors while preserving caller-owned fault injection and
lifecycle assertions. Estimated net reduction: 450-700 lines. Preserve promotion identity,
predecessor state, publication/disposal receipts, autosave quiescence, and pending-flight custody.

## PRE-003: Share Durable-Edit Test Fixtures

Syndic adoption, publication, history, historical-root, piece-tree, range-source, and materializer
tests repeat edit headers, ordered fragment chains, staging, and advancement. Full transaction
bodies at `tests/piece_tree.rs:797` and `tests/syndic_range_source.rs:401` are effectively identical.
Use a prepared-edit fixture with explicit predecessor/successor caret and selection and separate
begin/stage/advance/settle operations. Keep fault insertion, unadmitted-marker variants, per-step
record limits, and terminal assertions visible. Estimated net reduction: 250-350 lines.

## PRE-004: Consolidate Temporary Test Homes

Twenty-three TestHome definitions repeat unique-path creation and cleanup. Use one narrow helper
per crate, preserving exclusive creation/retry behavior and store-before-directory drop ordering
on Windows. Existing app tempdir support and Syndic support provide starting implementations.
Estimated net reduction: 260-350 lines; independent of PRE-003.

## PRE-005: Share Default Protocol Test Admission

App recovery-history, recovery-page, submitted-input, normal-terminal, steering, and provider tests
repeat initialization/config admission and default JSON replies. Share default response builders
and admission exchange with the existing reader passed explicitly. Reader Ping, closed-connection,
and error handling differ and must remain intentional. Estimated net reduction: 120-190 lines.

## PRE-006: Remove Source-Shape Assertions Covered By Behavioral Tests

The transcript source-assertion lead is superseded by complete review AS-009. Its net estimate
accounts for missing host/panel behavioral coverage and retained architectural exclusions. Do not
add this preliminary entry to AS-009. AX-018 completed the remaining context-compaction boundary
review and folds its evidence into AS-009 with zero additional savings.

## PRE-007: Reduce Corruption-Fixture Record Reconstruction

`crates/syndic-storage/src/test_faults/draft_piece_corruption.rs` repeatedly reconstructs complete
records to change one field. Keep corruption variants explicit and share only reconstruction.
Superseded by SC-007 after complete-body review: its concrete draft-piece subrange is 95-125 net
lines, included only in SC-007's total. Preserve malformed versus recomputed digests, distinct
aggregate corruption, overflow, ordering, envelope errors, and coordinated receipt authentication.

## PRE-008: Share Backend Control-Notification Parsing Mechanics

`crates/beryl-backend/src/incoming_json/provider/machine/` thread-closed, thread-status, turn-started,
and normal-terminal machines repeat fragmented scalar handling, identity accumulation, exact-name
matching, container transitions, and sink completion handling. Share a private control reader while
retaining notification-specific grammar. Estimated net reduction: 300-450 lines. Preserve wire
ordering, fragmented UTF-8, limits, terminal diagnostics, backpressure, and operation custody.
The earlier envelope-decoder cleanup is already complete and is not counted again.

## PRE-009: Share Syndic Authentication And Publication Predicates

`draft_piece/mutation.rs:700`, `:812`, transaction authentication, and `read.rs:1068` repeat root
and receipt validation through different readers. Publication/disposal repeat related history and
receipt predicates. Introduce narrow reader adapters and shared invariants with existing read order,
budgets, and caller-specific errors. Estimated combined net reduction: 250-360 lines. Persistence
corruption and receipt authentication checks must remain at every required boundary.

## PRE-010: Unify Durable-Job Transition Plumbing

`crates/beryl-state/src/durable_job/mutation/transition.rs:148` contains seven DomainMutation
implementations repeating lookup, revision validation, reservation, revision advancement, and index
publication. Use one closed action enum and one implementation while retaining the explicit valid
transition matrix. Estimated net reduction: 200-260 lines. This changes source API shape, not the
required lifecycle or stored format. Lack of current app callers does not make the documented
handoff semantics obsolete. Verify legal/illegal transitions, stale revisions, failed checkpoints,
terminal nonregression, and live-index agreement.

## PRE-011: Reuse State's Existing Binary Codec

Catalog and settings private Encoder/Decoder implementations duplicate `beryl-state/src/encoding.rs`.
Reuse its integer, UTF-8, cursor, exact-end, and path-flavor machinery with precise domain error
conversions. Estimated net reduction: 130-160 lines. Preserve field order, stored bytes, schema
refusal, bounds, truncation, invalid tags/UTF-8, and trailing-byte rejection.

## PRE-012: Simplify Theme Parsing Ownership

`beryl-state/src/theme/document.rs` and `manifest.rs` contain separate restricted TOML lexical and
parser implementations. Current-contract sharing may save 70-110 lines. An alternative replaces
roughly 1,000 lines of parser area with capped input acquisition, the existing TOML dependency, and
typed validation; provisional net saving 350-550 lines requires a prototype. Alternatives overlap.
Documents are capped at 256 KiB and manifests at 1 MiB/1,024 entries. Current authority explicitly
requires incremental parsing and forbids whole-document residency; changing that is an explicit
design proposal. Accepted grammar, diagnostics, caller allowances, page budgets, and finite
concurrency require investigation. The complete audit must evaluate current official dependency
evidence before endorsing this replacement.

## PRE-013: Reduce Transparent App Forwarding And Repeated Geometry

Composer service methods repeat slot locking, invocation, and error translation. Transcript
selection/menu/media adapters repeat realized-record lookup and the same geometry construction.
CAS shutdown settlement/draining and forwarding-hub admission repeat mechanics with meaningful
ordering and payload-custody differences. Four narrow refactors together were estimated at 190-320
net lines. Keep policy-bearing service methods, lock lifetime, interaction-specific eligibility,
ordinary versus terminal retirement ordering, and exact payload return ownership. Unmounted target
transcript source remains required and is not blanket dead-code deletion material.

## PRE-014: Consider A Simpler Notice Copy Contract

The notice detail implementation supports partial pointer/keyboard selection, navigation, selection
geometry, and caret reveal. AW-003 supersedes this preliminary estimate after full review: a
Copy Detail action could remove an estimated 330-430 net lines while
retaining shaping, scrolling, revision checks, and inert fencing. This changes the documented
feature/widget contract. Reusing the existing text-input dependency unchanged was previously ruled
out by inert and scroll-continuity gaps; any dependency replacement must account for fixing those.

## PRE-015: Reconsider Persistent Composer Complexity As A Design Choice

The large composer implementation supports arbitrary-size paged editing, authenticated roots,
durable history, immutable publication, pending activation, and externally settled ownership.
A bounded owned-buffer editor with bounded snapshot undo may substantially simplify that design,
but changes supported draft/history behavior. No net savings are claimed before a replacement is
specified. The complete audit must also investigate whether established data-structure crates can
retain the current behavior with materially less custom implementation.

## PRE-016: Revisit Previously Deferred Tree And Seal Ownership Consolidation

Prior work deferred sharing marker identity/rank tree balancing until the editor stabilizes and
replacing the global weak marker-seal registry with a single bootstrap-owned home service. These
remain investigation leads tied to paused ownership decisions. They are not newly discovered,
completed, or included in an immediate implementation budget. Trace their current callers and
controlling design before retaining or replacing them in the final proposal.

# Completed File Review Reports

## Model And Stream: MS-001 Through MS-008

Evidence maturity: `file_review`. Complete bodies of all 22 model/stream source and test files
(3,980 physical lines) were examined. The root auditor checked the exact inventory and selected
decision-relevant bodies before accepting the [coverage report](reviews/model-stream.tsv).
The [structured findings](reviews/model-stream-findings.json) contain evidence, alternatives,
contract implications, verification requirements, and official dependency sources.

- MS-001 proposes replacing nine manual model error implementations with the existing workspace
  `thiserror` 2.0.18 dependency, preserving enum shapes and exact messages: approximately 100 net lines.
- MS-002 proposes using a fallibly preallocated standard-library VecDeque inside the existing channel
  boundary, removing manual ring state and unnecessary message wrapping: approximately 30 net lines.
- MS-003 proposes removing page-lease generations and their permanent exhaustion failure, which
  have no production consumer. Existing byte, occupancy, release, and total-lease assertions can
  retain custody evidence: approximately 30 net lines. The public boundary removal remains a proposal.
- MS-004 shares page diagnostic construction and derives nongeneric Clone implementations:
  approximately 18 net lines.
- MS-005 simplifies repeated marker-digest test construction while retaining independent canonical
  byte vectors: approximately 25 net lines.
- MS-006 through MS-008 retain typed model domains, channel custody and observation semantics, and
  explicitly required range/replay interfaces. Unmounted required adapters are not obsolete merely
  because they currently have only test callers.

These are local review findings. The roughly 203-line estimate is not a global savings conclusion,
and no proposed change has been implemented or verified by execution.

## Executable Placeholder: EN-001

Evidence maturity: `file_review`. The complete three-line entry file, its manifest, and its package
authority were read. [Coverage](reviews/entry.tsv) and [finding](reviews/entry-findings.json) retain
the explicit incomplete-bootstrap compile error. Replacing it with an empty successful executable
would conceal paused work and is not a simplification. This audit does not establish that the
baseline builds into a complete runnable application.

## Backend Incoming JSON: BJ-001 Through BJ-005

Evidence maturity: `file_review`. All 59 files and 15,368 lines were examined in full. Root checked
the exact coverage set, finding associations, and the decision-relevant parser/driver bodies before
accepting [coverage](reviews/backend-json.tsv). [Structured findings](reviews/backend-json-findings.json)
retain exact source evidence, bounds, alternatives, overlap, and verification requirements.

- BJ-001 expands PRE-008 into shared fixed-byte/scalar/control mechanics: 300-450 estimated net lines.
- BJ-002 shares leased-page output mechanics between provider and dynamic-tool capture: 100-170
  estimated net lines. Steering capture has different verification/abandonment responsibilities.
- BJ-003 replaces compact-response continuation machines with synchronous pull parsing over the
  existing bounded-json recognizer: 1,800-2,600 estimated net lines. The source API yields one owned
  event with exact extents and supports the needed cursor; the Beryl handoff, borrowing, error
  precedence, and exact consumption still need a prototype. Provider direct-page capture stays on
  its existing event path. This is an architectural candidate, not a measured or verified patch.
- BJ-004 removes unsupported reasoning-effort string-array/object-map alternatives: 100-140
  estimated net lines. Exact pinned Codex source emits records. Existing positive fixtures accepting
  the other shapes conflict with that authority. This saving is included in BJ-003 if selected.
- BJ-005 retains the current recognizer and bounded number accumulator after comparing registry
  alternatives. serde_json needs intermediate scalar/name storage; struson's pull interface still
  materializes names/numbers; fast-float2 needs contiguous number input. No compatible net reduction
  was demonstrated from replacing those dependencies.

Reusable dependency evidence is retained for
[serde_json 1.0.151](../../memory/crates.io/serde_json/1.0.151/bounded-ingress-replacement.md),
[struson 0.7.2](../../memory/crates.io/struson/0.7.2/bounded-ingress-replacement.md), and
[fast-float2 0.2.4](../../memory/crates.io/fast-float2/0.2.4/incremental-number-replacement.md).
The existing error-derive option has a separate
[thiserror 2.0.18 note](../../memory/crates.io/thiserror/2.0.18/manual-error-simplification.md).

## Remaining Backend: BR-001 Through BR-010

Evidence maturity: `file_review`. All 137 remaining backend files and 29,552 lines were examined
in full. Root checked exact baseline coverage, finding associations including cross-references to
BJ findings, and selected reader/metric bodies before accepting [coverage](reviews/backend-rest.tsv).
The [structured findings](reviews/backend-rest-findings.json) retain complete evidence and verification.

- BR-001 shares the neutral response wait across six method owners: 150-220 estimated net lines.
  Caller-specific identity, dispatch and echo-completion rules remain separate.
- BR-002 replaces a boxed two-stack approval FIFO with explicitly bounded VecDeque: 35-55 lines.
- BR-003 shares WebSocket writer setup and failure normalization: 100-150 estimated net lines.
  Source-failure precedence, actual first-byte dispatch and partial-write poisoning remain.
- BR-004 uses stack-owned synchronous reader bookkeeping and removes inert state: 65-110 lines.
  A saturating length can never exceed its sole usize::MAX budget; separate real metrics remain.
  Removing a constant-false retained-result metric also requires changing app test interfaces at
  connection/driver.rs, test_faults/provider.rs, normal_terminal/verification.rs and
  provider_residency/fixture.rs. Those app changes are excluded from this local estimate.
- BR-005 shares successful authenticated initialization fixtures: 220-330 estimated net lines.
  Fault placement and incompatible/uninitialized handshakes remain explicit.
- BR-006 removes exact helper-spelling and statement-order source assertions: 30-50 lines. Retain
  the meaningful no-DOM, no-probe, no-target-touching and no-coarse-cleanup exclusions.
- BR-007 uses existing thiserror and Debug derives for exact conventional wrappers: 80-110 lines.
  Redacted leaves and concrete source-chain behavior remain.
- BR-008 derives fixed injection wire shapes around the existing streamed text leaf: 35-55 lines.
- BR-009 retains specialized process and fragmented WebSocket ownership after investigating
  [process-wrap 10.0.0](../../memory/crates.io/process-wrap/10.0.0/backend-process-ownership.md) and
  [tungstenite 0.30.0](../../memory/crates.io/tungstenite/0.30.0/incremental-canonical-ingress.md).
- BR-010 records typed Serde as a conditional alternative for explicitly restricted ancillary
  responses. A new input cap loses formerly accepted discarded metadata and requires a classifier
  prefix handoff, field-order/duplicate policy, allocation accounting and full drain/poison behavior.
  No cap for canonical streams, user input or recovery history is proposed. This overlaps BJ-003
  and is excluded from totals; a sample 1 MiB cap is a prototype question, not an established limit.

The eight local proposals total 715-1,080 estimated net lines, before whole-codebase reconciliation.
No build or tests were run and no implementation changed.

## Home Store: HS-001 Through HS-010

Evidence maturity: `file_review`. All 99 files and 30,374 lines were examined, including all
conditional bodies and tests; three files contain declarations only. Root checked the exact
baseline coverage set, finding associations and selected proof/footprint bodies before accepting
[coverage](reviews/home-store.tsv). [Structured findings](reviews/home-store-findings.json) retain
the complete evidence, contract implications and verification requirements.

- HS-001 shares validated recovery declarations and family opening, preserving all-domain preflight
  before any runtime attachment callback: 100-180 estimated net lines.
- HS-002 replaces repeated sealed receipt facts with the existing unique immutable command binding:
  75-115 estimated net lines. Store/generation admission and independent expected/observed correlation
  comparison remain. This needs a precise proof-contract clarification and substitution tests.
- HS-003 replaces inaccessible collision archives with a compact closed-scope marker: 150-260
  estimated net lines. This explicitly changes archive-retention authority. Closed scopes, completed
  flights, home lifetime and semantic successor checks remain.
- HS-004 shares bounded physical reads and preserves classified error provenance: 40-85 estimated
  net lines. Correcting currently inconsistent classification is a behavior fix as well as reuse.
- HS-005 carries one writer snapshot through admission: 40-85 estimated net lines. Preparing every
  mutation and validating receipt revision capacity before any contribution remains mandatory.
- HS-006 shares staged theme-file publication mechanics: 100-180 estimated net lines. Manifest-last
  ordering, directory synchronization outcomes and inert failure residue remain.
- HS-007 removes a literal-only metadata-envelope test and shares fixtures: 230-370 estimated net
  lines. Actual encoder capacity and physical fault scenarios remain independently tested.
- HS-008 and HS-010 retain typed custody, two-stage preparation, exact storage formats, and required
  durable publication. Generic storage or atomic-write crates did not demonstrate a smaller fit.
- HS-009 retains the bounded watcher. [notify 8.2.0 investigation](../../memory/crates.io/notify/8.2.0/bounded-theme-watcher.md)
  found an unresolved Windows worker-termination guarantee and filesystem coverage decisions.

## State: ST-001 Through ST-014

Evidence maturity: `file_review`. All 125 files and 41,848 lines were examined in full. Root checked
the exact baseline coverage set, finding associations and selected reconciliation/test bodies before
accepting [coverage](reviews/state.tsv). [Structured findings](reviews/state-findings.json) contain
source locations, alternative costs and per-proposal verification.

- ST-001 shares fixed-domain registration and receipt adapters: 180-260 estimated net lines.
- ST-002 uses existing thiserror derives with exact source-chain exceptions: 600-900 estimated net
  lines, excluding errors removed by other proposals.
- ST-003 reuses the existing binary encoder/decoder in settings and catalog: 130-190 estimated net
  lines. Persisted bytes and typed error categories remain unchanged.
- ST-004 shares test plumbing: 650-1,050 estimated net lines. It also identifies a serialized
  spawn/join sequence posing as a race and a digest comparison whose changed argument is ignored.
- ST-005 represents seven durable-job transitions with one closed mutation enum: 200-260 estimated
  net lines. Lifecycle predicates and the independent failure compatibility matrix remain explicit.
- ST-006 shares paginated exhaustive scan mechanics: 100-180 estimated net lines after adapters.
  Per-domain invariants and special bounded acquisition loops remain local.
- ST-007 shares catalog claim checks and consumes checked rows instead of cloning them: 35-60 lines.
- ST-008 retains the exact Unicode 17 NFKC_Casefold implementation. The
  [ICU4X investigation](../../memory/crates.io/icu_normalizer/2.3.0/exact-catalog-casefold.md)
  did not prove an equivalent replacement.
- ST-009 shares session-window removal publication: 35-55 estimated net lines, keeping distinct
  ordinary-removal and abandonment admission.
- ST-010 shares asset evidence encoding, checked proof construction and point limits: 70-110 lines.
- ST-011 shares equivalent theme string/lexical primitives: 70-110 estimated net lines.
- ST-012 carries one private theme execution context and checked manifest reader: 150-240 lines.
- ST-013 gives pending reconciliation outcomes complete private payloads and validates expected
  publication before side effects: 80-130 estimated net lines.
- ST-014 is an alternative to ST-011: replace theme-document parsing with existing toml/serde at the
  fixed 256 KiB input boundary, for 350-550 estimated net lines. This requires explicit authority
  changes for whole-document allocation, concurrency, admitted syntax, and InstalledLoad handling;
  manifest parsing remains streaming. The [TOML investigation](../../memory/crates.io/toml/0.9.12+spec-1.1.0/capped-theme-document.md)
  records the exact API and required allocation/compatibility prototype.

These are audit proposals, not implemented savings. Estimates remain subject to overlap and
whole-codebase review; retained alternatives are not counted as deletions.

## Syndic Schema, Codecs And Values: SS-001 Through SS-010

Evidence maturity: `file_review`. All 107 assigned files and 24,600 lines were examined in full.
Root checked exact baseline coverage, finding associations, selected key/resource decoder bodies,
and the input-gate authority against the paused plan before accepting
[coverage](reviews/syndic-schema.tsv). [Structured findings](reviews/syndic-schema-findings.json)
retain source evidence, caller dependencies, contract implications and verification requirements.

- SS-001 declares closed scalar tag pairs once and generates both matches: approximately 220 net
  lines. Preserve explicit persisted bytes and retired tags; payload-bearing codecs remain explicit.
- SS-002 reuses existing compatible counter and nonce macros: approximately 90 net lines. Nominal
  types, error labels and overflow behavior remain distinct where required.
- SS-003 uses the existing model discussion owner as the context-envelope key: approximately 35
  net lines, including removal of the private duplicate and its conversions. Ordered bytes and key
  diagnostic labels remain unchanged. Read/mutation/validation/test-fault callers need coordinated edits.
- SS-004 updates private cloned successor records then applies their shared validator: approximately
  120 net lines. Construction and every successor retain invariant checks; catalog initial mapping
  reuses its existing source constructor.
- SS-005 records correlated option fields as a representation candidate, with no counted estimate.
  Resource variants, route intervals and stop admission can express their complete payloads directly,
  but public getter adapters and full caller costs still need assessment.
- SS-006 derives conventional CodecError presentation with existing thiserror: approximately 15 lines.
- SS-007 shares exact content-span wire bodies and existing option/bool primitives: approximately
  40 net lines, retaining context-specific diagnostics and EOF checks.
- SS-008 removes temporary resource-decoder string allocations. It is an allocation improvement
  with no net line estimate; borrowed option parsing may need explicit input-lifetime plumbing.
- SS-009 defers [Postcard 1.1.3](../../memory/crates.io/postcard/1.1.3/syndic-record-value-codecs.md)
  to a separately authorized new format. Its default tags, lengths and integers are not V7 bytes.
  A complete adapter and domain-validation prototype is required before claiming savings.
- SS-010 records current input-gate V4 implementation and tests versus target V5/RepairRequired.
  Root confirmed repair-required successor gates and durable request claims are pending in
  [Checkpoint 5](../../rework/beryl-home/REWORK.md), while the root plan pauses Checkpoint 4.
  This is a known baseline-to-target gap to reconcile with planned work, not a test-version-only fix
  or a simplification deletion.

The six counted local proposals estimate 411-640 net lines, midpoint approximately 520. The other
entries are uncounted; no serializer migration, implementation, build or test was performed.

## Application Shell: AS-001 Through AS-009

Evidence maturity: `file_review`. All 117 assigned files and 36,888 lines were examined in full,
including 88 source files and 29 tests. Root checked exact coverage and finding associations,
selected resident-snapshot/status bodies and unused virtual-list API references before accepting
[coverage](reviews/app-shell.tsv). [Structured findings](reviews/app-shell-findings.json) retain
complete evidence, costs, caller dependencies and verification requirements.

- AS-001 shares coalesced cache lifecycle and removes duplicate retained request payloads: 160-280
  estimated net lines. Separate syntax plain-while-pending and coherent old projection behavior,
  request fencing, worker-held byte accounting and independent owner budgets remain.
- AS-002 represents code lines by ranges over one immutable source and shares span projection:
  70-130 estimated net lines. CRLF, trailing empty lines, wrap preference and copy-source identity
  remain; the full target wrap index is not claimed implemented by this cleanup.
- AS-003 shares syntax assertions: 50-75 estimated net lines, preserving distinct dialect/error cases.
- AS-004 retains existing parser adapters after investigating
  [Syntect 5.3.0](../../memory/crates.io/syntect/5.3.0/bounded-code-panel-highlighting.md).
  No grammar, long-line, parser-state or net-cost fit has been established for adoption.
- AS-005 removes unused whole-list/Infer sizing modes from the app-private virtual list: 90-130
  estimated net lines. Bounded visible measurement, anchor/focus and virtual-tail behavior remain.
- AS-006 shares realized-record lookup and geometry validation: 65-105 estimated net lines. Each
  selection, quote, menu and media action keeps its own eligibility and stale-source rejection.
- AS-007 removes copied collection lengths and manual default plumbing, and avoids full resident
  payload clones for content-free status/diagnostics: 70-115 estimated net lines. Real byte/capacity,
  pin, generation and worker-retention accounting remain explicit.
- AS-008 shares resident fixture construction and provider dispatch: 250-400 estimated net lines.
  Scenario-specific revision, provenance, budget, pin and late-response inputs remain visible.
- AS-009 replaces positive implementation-string requirements with focused behavior checks and
  compact architectural exclusions: 220-320 estimated net lines. It supersedes PRE-006's transcript
  portion. Host/panel dispatch coverage must be supplied where current pure-core tests do not reach.

The eight proposals total 975-1,555 estimated net lines. Required immutable-page, paged activity and
notification-audio work remains pending in the existing rework; no entire prototype is treated as
dead code. The already-tracked audio lane replacement must retain its persistent playback objects
because earlier evidence showed cold-first-sound truncation without them. No product changes,
builds, tests, installations or background resources were created by this review.

## Syndic Provider Items And Observations: SP-001 Through SP-007

Evidence maturity: `file_review`. All 97 assigned files and 23,166 lines were examined: 88 files
with full implementation bodies and nine declaration-only files. Root checked exact baseline
coverage, finding associations and the repeated preparation-encode body before accepting
[coverage](reviews/syndic-provider.tsv). [Structured findings](reviews/syndic-provider-findings.json)
retain source evidence, preserved semantics and verification requirements.

- SP-001 shares PIV1 byte/chunk/hash/span writing between typed and observation encoders: 100-170
  estimated net lines. Their source traversals remain distinct; exact tags, flush timing, digest
  domains, coordinates and error conversion remain.
- SP-002 shares the checked lifecycle transition with observation preparation: 55-80 estimated
  net lines. Error precedence, timestamps, history support and unchanged state on rejection remain.
- SP-003 retains the sole optional narrative span during the first encode and derives narrative
  metadata after the frame digest is known: 180-255 estimated net lines across both preparation paths.
  The original compiler estimate is 90-130; SX adds 90-125 for mutation preparation once. Remove the repeated encode
  and sink, preserving span rejection, sealed replay validation, staged identity and exact completion
  comparison. Durable staging encoding remains in mutation/provider_frame/stage.rs.
- SP-004 derives replay text summaries through the existing text traversal: 25-40 estimated net
  lines, preserving selector errors, output-error distinction and validation through EOF.
- SP-005 shares thin successful event/commit fixture helpers: 350-550 estimated net lines. Keep
  independent expected schemas/digests, invalid calls, fault timing and reconciliation custody.
  Temp-directory cleanup and every scenario deletion are excluded from this estimate.
- SP-006 retains independent production streaming validation, bounded test decoding and exact typed
  provider values. Bincode 3.0.0 was [rejected at maintenance screening](../../memory/crates.io/bincode/3.0.0/provider-codec-screen.md);
  generic formats have not established compatible PIV1 semantics or bounded allocation.
- SP-007 retains constant-state locator validation after inspecting
  [url 2.5.8](../../memory/crates.io/url/2.5.8/incremental-provider-locators.md). Its whole owned
  string and WHATWG normalization do not replace exact incremental locator validation.

The five proposals total 710-1,095 estimated net lines including the later SX mutation extension.
No source, tests, manifests or runtime
state changed; no build, test, install or dependency upgrade was performed.

## Application Services: AU-001 Through AU-009

Evidence maturity: `file_review`. All 54 assigned files and 18,252 lines were examined: 50 with
full implementation bodies and four declaration-only files. Root verified exact baseline coverage,
finding associations and the paired theme retirement bodies before accepting
[coverage](reviews/app-services.tsv). [Structured findings](reviews/app-services-findings.json)
record evidence, preserved contracts and future verification.

- AU-001 shares private bounded dynamic-tool wire helpers: 150-220 estimated net lines. Namespace,
  missing/null, clamping/rejection, string limits and error policies remain explicit at callers.
- AU-002 shares small diagnostic ring mechanics only if the result is smaller: 20-40 estimated net
  lines. Typed snapshots, independent sequences and post-filter continuity remain.
- AU-003 defines the common memory log field list once: 60-85 estimated net lines. Both existing
  event schemas and disabled-log laziness remain; deleting flat diagnostic fields is excluded.
- AU-004 uses existing thiserror 2.0.18 for bounded errors: 45-70 estimated net lines, preserving
  messages and source topology. No dependency addition or upgrade is required.
- AU-005 delegates duplicate theme retirement and removes discarded invalid outcomes: 20-30
  estimated net lines. Shutdown order, retained diagnostics and publication identities remain.
- AU-006 shares theme runtime fixtures and paired scenario setup: 120-190 estimated net lines,
  preserving real GPUI paint and editor continuity, failure cuts and retry/reread differences.
- AU-007 shares pure execution-binding validation and catalog fact projection: 55-75 estimated net
  lines. Home fences, atomic command assembly and acquisition custody remain with their owners.
- AU-008 shares acquisition/abandonment fixtures: 170-220 estimated net lines. Actual crash/reopen,
  subprocess reentry, registration, fault timing and process continuity remain explicit.
- AU-009 retains diagnostic child process ownership. The
  [process-wrap 10.0.0 investigation](../../memory/crates.io/process-wrap/10.0.0/backend-process-ownership.md)
  also applies here: the inspected synchronous API cannot configure required kernel kill-on-close.

The eight proposals total 640-930 estimated net lines, excluding AS-007 shell projection changes.
No source, tests, manifests or runtime state changed; no build, test or installation was performed.

## Syndic Marker Admission: SA-001 Through SA-011

Evidence maturity: `file_review`. All 49 assigned files and 23,728 lines were examined: 45 with
full implementation bodies and four declaration-only files. Root verified exact baseline coverage,
finding associations, readiness attempt ownership and the duplicate test module declaration before
accepting [coverage](reviews/syndic-admission.tsv).
[Structured findings](reviews/syndic-admission-findings.json) record evidence and verification.

- SA-001 stores a single private canonical Parts payload per admission head/receipt and borrows it
  for encoding/validation: 110-170 estimated net lines. Exact bytes, opacity, digest checking and
  any required Debug presentation remain; boxed closure copies solely for conversion disappear.
- SA-002 shares authenticated root/child predicates: 35-80 estimated net lines. Each traversal keeps
  its own selection, fanout policy, error mapping and path-capture requirements.
- SA-003 shares exact metadata charge exchange and successor finalization: 140-240 estimated net
  lines. Operation-specific reads, lifecycle, arithmetic refusal order and command limits remain.
  Removing the repeated writer read requires preserving the authority's read-accounting semantics.
- SA-004 makes four readiness-attempt fields mandatory while keeping the extractable command
  optional: 35-65 estimated net lines. Consume-by-value retains exact receipt and rollback custody.
- SA-005 shares one private cancel/fail/supersede seal mutation: 85-130 estimated net lines. Named
  public owners, permitted terminal outcomes, identity collision checks and replay behavior remain.
- SA-006 shares bounded dual-index successor assembly: 30-65 estimated net lines, preserving
  source-before-target ordering, authentication, predecessor protection and exact byte charges.
- SA-007 shares marker readiness and admitted staging fixture mechanics: 140-240 estimated net
  lines. Fault timelines, independent canonical encoders and distinct admission entry paths remain.
- SA-008 uses existing thiserror for four boundary errors: 45-80 estimated net lines, preserving
  every message, conversion, source chain and callback classification.
- SA-009 rejects [im 15.1.0 for durable admission indexes](../../memory/crates.io/im/15.1.0/durable-admission-index-screen.md).
  An in-memory immutable map does not replace authenticated durable node I/O and exact replay.
- SA-010 retains independent preflight and execution-time proof observations, canonical marker
  commitments, occurrence multiplicity and live generation/terminal custody.
- SA-011 records duplicate `mod support` declarations in the accepted-proof integration test.
  Rename the fixture-specific module while retaining common support before relying on that target.
  This is static source evidence, not a compilation result or an implementation performed here.

The eight reduction proposals total 620-1,070 estimated net lines (central estimate 825). The test
wiring correction and retained/rejected alternatives carry zero savings. No source, tests,
manifests or runtime state changed; no build, test or installation was performed.

## Syndic Queries And Validation: SQ-001 Through SQ-R02

Evidence maturity: `file_review`. All 133 assigned files and 36,929 lines were examined: 131 with
full implementation bodies and two declaration-only modules. Root checked the exact frozen set,
status/depth/IDs and the complete delivering-generation fixture before accepting
[coverage](reviews/syndic-queries.tsv). [Structured findings](reviews/syndic-queries-findings.json)
retain relationship, temporal, error and verification requirements. Report dispositions
`recommended`, `retain` and `defer` map to register `proposed`, `retained` and `deferred`.

- SQ-001 shares ready/delivering steering acquisition and execution predicates: 250-350 estimated
  net lines. Each API retains its lifecycle asymmetries, diagnostics, mutable-anchor confirmation
  and twelve-point-read bound; no membership scan is introduced.
- SQ-002 shares encoded-content byte filling through bounded output slices: 140-220 estimated net
  lines. Store/DomainReader adapters retain accounting, digest/identity checks, gap rejection and
  each caller's size limit. Arbitrary encoded bytes remain separate from UTF8-clipped text pages.
- SQ-003 shares manifest-stabilized text page orchestration: 65-100 estimated net lines, preserving
  validation order, marker policy, empty pages, continuation and residency release timing.
- SQ-004 shares staged/published narrative-span authentication: 40-65 estimated net lines. Their
  completion rules, first-generation requirements and source acquisition remain at the callers.
- SQ-005 reuses the existing internal Read/Invariant validation error: 55-85 estimated net lines.
  Physical Read provenance and the two mutation boundary mappings remain explicit; the I/O error
  side channel is retained. SX confirms the two mutation boundary mappings and their typed errors.
- SQ-006 removes an identity rewrite loop from the delivering-read fixture: 105-115 estimated net
  lines. Construct the final summary revision directly, retaining all eight record values,
  384-member generation, commit chunking and twelve-read assertion.
- SQ-007 shares revision-fenced accepted-source page transport: 35-55 estimated net lines,
  preserving public cursor/result types, stale/invalid diagnostics, limits and source revision.
- SQ-R01 retains exhaustive physical-corruption scans, orphan reverse-index checks and historical
  descendant boundaries. Ordinary read methods and whole-home collections do not replace them.
- SQ-R02 defers broad recovery/stop consolidation and cross-scope promotion-record/activity-phase
  reuse. Fault observations at specific read boundaries and independently absent corrupt records
  remain meaningful. Mutation review must confirm any later pure-helper savings, counted once.

The seven proposals total 690-990 estimated net lines. No dependency addition was justified by
this cohort. No source, tests, manifests or runtime state changed; no build or test was performed.

## Syndic Durable Draft Tree: DT-001 Through DT-007

Evidence maturity: `file_review`. All 18 files and 19,626 lines were read with full bodies,
conditional code and macros. Final follow-up checked caller impact, shared support compatibility
and estimates. Root verified exact baseline coverage, depth/IDs and the nested receipt payload
before accepting [coverage](reviews/syndic-draft-tree.tsv).
[Structured findings](reviews/syndic-draft-tree-findings.json) retain detailed evidence.

- DT-001 flattens the candidate-session record into four explicit variants: 60-100 estimated net
  lines. Preserve opaque open-receipt APIs, boxed large variants, exact V7 tags and validation.
  Coordinate the public Rust record enum and exhaustive caller updates; this estimate includes
  immediate publication/session downcasts and must not be counted again in their review.
- DT-002 shares the eleven-field private progress payload and receipt digest traversal: 60-100
  estimated net lines. Independently durable head and receipt records, public accessors, both
  encodings and all nonshared authentication remain. Mutation caller header-copy cleanup is excluded.
- DT-003 reuses build_roots during final root construction: 60-90 estimated net lines, preserving
  exact allocation order, node identities, canonical summaries and charged work.
- DT-005 shares forward/reverse sequence cursor mechanics: 40-80 estimated net lines. Direction,
  scalar anchoring, exclusive continuation, returned order and bounded reads remain explicit.
- DT-006 reuses existing test command, marker-limit and publication-capture helpers: 70-120 estimated
  net lines. Keep label-protection execution identities, limit=1 fixture behavior, real home
  operations and every custody/corruption/acquisition assertion. Coordinate durable_builder support.
- DT-004 confirms PRE-001's no-effect Removing continuation. Its 40-100 conditional estimate is
  nonadditive and excluded from the subtotal: serialized/hash-bound shapes, transition history
  and diagnostics need an explicit schema decision before any removal.
- DT-007 rejects [Ropey 1.6.1 as a durable piece-tree replacement](../../memory/crates.io/ropey/1.6.1/durable-piece-tree-screen.md)
  and reuses the im negative screen. Neither provides the authenticated durable-node and replay
  contract. No required tree, codec family or entire reviewed slice is counted as removable.

The five behavior-preserving proposals total 290-490 estimated net lines, with coordinated Rust
API and shared test-support changes where stated. Final synthesis must reconcile sibling caller
overlap. No source, tests, manifests or runtime state changed; no build or test was performed.

## Application Connections: AC-001 Through AC-011

Evidence maturity: `file_review`. All 105 assigned files and 31,623 lines were examined: 102 with
full bodies and three declaration-only files. Root checked the exact baseline set, reservation
representation and shared broker context before accepting [coverage](reviews/app-connection.tsv).
[Structured findings](reviews/app-connection-findings.json) retain detailed ordering, custody,
error and verification requirements. Their local `subsystem_review` labels do not complete the
whole application subsystem review, which remains pending the other application cohorts.

- AC-001 represents live cleanup/promotion reservations by exact identity and shares settlement:
  80-120 estimated net lines. Distinct move-only owners, lock ordering, election and retirement stay.
- AC-002 shares ordered driver post-operation settlement and matching invalidation classifiers:
  35-60 estimated net lines. Authorization, source ownership, classification and error precedence stay.
- AC-003 shares shutdown caching and mechanical joins: 40-70 estimated net lines. Ordinary and
  terminal coordinators retain their different authority-retirement order and sticky failure.
- AC-004 removes an unused private dispatch adapter and unread committer fields: 15-25 estimated
  net lines. Actual generation/command guards and immediate staging custody remain.
- AC-006 shares immutable broker context: 45-75 estimated net lines. The ingester retains its
  receiver and private mutable state; context must not own its worker handle or create a cycle.
- AC-008 shares router test activation and exact loss-request waits: 65-100 estimated net lines.
  Active-only and pending-activation constructors, interleavings and intermediate assertions stay.
- AC-009 shares checked-frame test assertions: 30-45 estimated net lines, preserving success/loss
  provenance, binding outcomes, finalization and exact event-count differences.
- AC-010 shares broker test environment construction and exact shutdown checks: 100-160 estimated
  net lines. Durable scenario setup and registration order remain explicit at callers.
- AC-011 shares small WebSocket fixture helpers: 50-85 estimated net lines. Request IDs,
  Ping/Pong behavior, timeout and strict frame checks remain explicit; fragmented generation stays.
- AC-005 defers split broker construction changes with zero savings. Fresh unpublished candidate
  construction remains required, and dormant activation capability cannot yet be called dead code.
- AC-007 defers neutral provider observation vocabulary with zero savings until cross-package
  synthesis resolves type ownership, durable format independence and replacement cost.

The nine proposals total 460-740 estimated net lines: 215-350 production and 245-390 test support.
BR-004 ingress metric savings are excluded. No source, tests, manifests or runtime state changed;
no build, test or installation was performed.

## Application Composer Host: AP-001 Through AP-008

Evidence maturity: `file_review`. All 57 assigned files and 20,060 lines were examined with full
bodies, conditional code and macros. Root checked exact baseline coverage and the request-number
defect against its current authority and rejection predicate before accepting
[coverage](reviews/app-composer.tsv). [Structured findings](reviews/app-composer-findings.json)
retain detailed custody, bounds, overlap and verification requirements.

- AP-001 shares retained-page preparation and its private translation-progress value: 55-85
  estimated net lines. Widget/internal kind, separate lanes, exact retry identity and custody stay.
- AP-002 shares publication failure classification and submission result application: 20-40
  estimated net lines. Autosave/flush success policies, timers, close gates and recapture stay.
- AP-003 shares initial activation validation and terminal cleanup: 25-45 estimated net lines.
  Bounded demand order, production open-session custody, cancellation and fault timing stay.
- AP-004 shares marker-seal completion and release tails: 20-35 estimated net lines. Existing
  terminal intents skip fresh authentication as required; relock ordering and Asset custody stay.
- AP-005 shares marker-seal/session/asset test mechanics: 130-210 estimated net lines, retaining
  separate sidecar admission and metadata publication, deliberate payloads and independent oracles.
- AP-006 shares test page submission and publication-drive mechanics: 170-290 estimated net lines.
  Explicit extents, predecessor identity, two lanes, readiness custody and fault scenarios remain.
- AP-007 records a correctness defect with zero savings. History and edit settlement reset the
  request high-water mark while retaining the host generation/session. Existing authority requires
  a monotonic sequence across these adoptions. Preserve pending cancellation and stale completion
  checks while correcting the reset in later authorized implementation; prove same-generation
  rejection and fresh-generation restart separately.
- AP-008 excludes already-mandated process-global marker-seal registry removal from new savings.
  Graph-owned service injection and shared capacity/retirement behavior remain required work.

The six proposals total 420-705 estimated net lines (central estimate 560). Storage admission/tree,
external test support and required service-construction changes are excluded. No new dependency
was justified. No source, tests, manifests or runtime state changed; no build or test was performed.

## Syndic Draft Operations: DP-001 Through DP-010

Evidence maturity: `file_review`. All 56 assigned files and 24,447 lines were examined: 53 with
full bodies and three declaration-only files. Root checked exact baseline coverage, receipt-prefix
hashing and the session-read ordering defect before accepting
[coverage](reviews/syndic-draft-operations.tsv).
[Structured findings](reviews/syndic-draft-operations-findings.json) retain detailed evidence.
Their local `subsystem_review` labels do not complete the remaining Syndic subsystem review.

- DP-001 shares initial build construction and immutable-header carry-forward: 110-170 estimated
  net lines. Exact V7 bytes, writer presence and independent receipt authentication remain.
- DP-002 shares six authenticated staging head/receipt construction sequences: 170-260 estimated
  net lines, including removal of receipt reconstruction that zeroes an excluded digest suffix.
  Independent records, hash domains, canonical bytes and operation-specific custody checks remain.
- DP-003 shares seven-field immutable session-opening equality: 25-40 estimated net lines.
  Live, captured, historical, published and disposed predicates retain their distinct checks.
- DP-004 reuses the prepared two-record disposal writer: 15-25 estimated net lines. Fresh
  abandonment and saved-checkpoint disposal retain separate admission and history semantics.
- DP-005 shares terminal membership and committed-writer finalization mechanics: 35-55 estimated
  net lines, preserving exact attachments, generation, outcome and local-custody error distinctions.
- DP-006 uses existing [thiserror 2.0.18](../../memory/crates.io/thiserror/2.0.18/manual-error-simplification.md)
  for three errors: 40-60 estimated net lines. Session/publication keep their current absent source
  chains and manual conversions; the source-returning preparation error is excluded.
- DP-007 shares candidate/staging test construction: 330-520 estimated net lines. Identity seeds,
  history policy, directed positions, acquisition limits, fault cuts and independent oracles remain.
- DP-008 removes three unreferenced private adoption-test helpers: 81-88 estimated net lines.
  Their 81 body lines contain no test cases; remaining position recording stays.
- DP-009 retains metered acquisition, independent durable records, terminal evidence variants and
  test-only authorization. Unmetered readers or caller reconstruction would lose required checks.
- DP-010 records a static correctness defect with zero savings. The session reader observes its
  head twice before acquiring dependent records. A valid intervening update can therefore produce
  InvariantFailure rather than ConcurrentChange. Bracket the bounded closure read with the head
  observations in later implementation; preserve stable corruption and real storage errors.

The eight proposals total 806-1,218 estimated net lines. DT/SA model and receipt work, external
support changes and shared TempDir savings are excluded. No new dependency investigation was
needed. No source, tests, manifests or runtime state changed; no build or test was performed.

## Application Mounted Composer: AM-001 Through AM-009

Evidence maturity: `file_review`. All 86 assigned files and 35,680 lines were examined: 84 with
full bodies and two declaration-only test entrypoints. Root checked exact baseline coverage,
the allocator reset and duplicate module declaration before accepting [coverage](reviews/app-mount.tsv).
[Structured findings](reviews/app-mount-findings.json) retain detailed evidence and overlap rules.
Their local `subsystem_review` labels do not complete the remaining application subsystem review.

- AM-001 shares pure seed/dispatch request translation: 60-95 estimated net lines. Existing response
  tags may replace a duplicate seed enum; seed eligibility, purpose and range checks remain.
- AM-002 shares native recovery prompt presentation between rendering and diagnostics: 100-160
  estimated net lines. Preserve displayed copy, focus, keyboard eligibility and disabled reasons;
  diagnostics should describe the actual presented state.
- AM-003 shares seven residency quantities through distinct bound/usage wrappers: 20-35 estimated
  net lines. Checked arithmetic, componentwise admission and public diagnostics remain.
- AM-004 shares the test-fault gate primitive: 90-140 estimated net lines, retaining named handles,
  explicit release, wake ordering, installation rules and each distinct fault location.
- AM-005 shares mounted widget configuration and construction fixtures: 350-600 estimated net
  lines. Explicit budgets, identities, clipboard policy, rendering roots and scenario setup remain.
- AM-006 shares streamed durable draft seeding: 180-260 estimated net lines. Chunk generation,
  independent byte expectations, positions, bounded retries and publication evidence remain.
- AM-007 extends AP-007's single correctness finding to the mounted dispatcher, which also resets
  request numbering on binding replacement. Count no extra savings or independent defect.
- AM-008 records duplicate `mod support` declarations under test-faults with zero savings. Give
  native marker-readiness and slot-fixture modules distinct names before compiling that target;
  this is static source evidence, not a reported compiler run.
- AM-009 retains distinct mount, widget, service and cleanup ownership. Their lifetimes differ,
  including resident close and source cleanup after route loss. A general lifecycle framework
  has no established net benefit at this boundary.

The six proposals total 800-1,290 estimated net lines (central estimate 1,045). AP asset/host fixture
work, TestHome construction, production storage and required graph ownership changes are excluded.
No substantial dependency replacement was justified. No source, tests, manifests or runtime state
changed; no build, test or GUI launch was performed.

## Application Input And Replay: AI-001 Through AI-014

Evidence maturity: `file_review`. All 117 assigned files and 24,256 lines were examined: 110 with
full bodies and seven declaration-only files. Root checked exact baseline coverage, the replay
identity conversion and active fixture imports before accepting [coverage](reviews/app-input.tsv).
[Structured findings](reviews/app-input-findings.json) retain detailed evidence. Root corrected
the replay enum count to 24 variants; estimates remain based on the inspected source bodies.
Their local `subsystem_review` labels do not complete the remaining application subsystem review.

- AI-001 removes impossible Some/true promotion states: 12-22 estimated net lines, retaining exact
  successful-command reconciliation, fatal Prior/Collision and reservation release ordering.
- AI-002 shares failure traversal: 150-240 estimated net lines. Keep current-health-loss and
  cut-correlation as separate predicates with their existing generation and gate-state semantics.
- AI-003 shares steering attempt checks: 88-107 estimated net lines, preserving every check's
  position and the separate failure-observation method.
- AI-004 shares three ordinary scheduler worker launch tails: 35-55 estimated net lines. Closure
  ownership, panic mapping, completion-before-wake, spawn failure and joins remain.
- AI-005 derives the empty scheduler diagnostics state: 35-45 estimated net lines. Initial enum
  defaults, signal masks, counters and transition behavior remain identical.
- AI-006 removes unused steering publication forwarding: 40-70 estimated net lines. Construct
  current commands at callers, retaining outcome mapping and immediate indeterminate custody.
- AI-007 shares the canonical 24-variant replay error: 120-170 estimated net lines. Preserve
  accepted public names, Display/source behavior and the real ordinary-execution classification.
- AI-008 deletes an unreferenced six-function freeze-comparison group: 105-115 estimated net lines.
  Live freeze/current-command reconciliation remains; no specific existing rework deletion covers it.
- AI-009 shares running/parked execution payload: 40-65 estimated net lines. Distinct types remain,
  parking releases only the worker permit, and field destruction must preserve custody order.
- AI-010 narrows live capture to its known exact-response caller: 10-25 estimated net lines.
  Activation errors and genuine completion-unknown convergence remain separate and unchanged.
- AI-011 shares scheduler admission fixture setup: 80-130 estimated net lines. Scenario-owned
  faults, barriers, waits, shutdown points and resource-return assertions stay explicit.
- AI-012 shares promotion command/assertion mechanics: 80-130 estimated net lines. Intentional
  build/execution failures, independent durable state and indeterminate outcomes remain visible.
- AI-013 deletes two unmounted promotion fixture modules: exactly 415 physical lines, with no
  replacement or runnable test removal. Recheck module/include/target references at implementation.
- AI-014 retains bounded replay and independent wire evidence: generation/owner rechecks, page
  limits, fragmentation, masking, cutoffs and non-idempotent outcomes remain required.

The thirteen proposals total 1,210-1,589 estimated net lines: 635-914 production and 575-675 tests.
Existing thiserror suffices for the canonical error; no new dependency was justified. No source,
tests, manifests or runtime state changed; no build or test was performed.

## Syndic Lifecycle Mutations: SL-001 Through SL-R01

Evidence maturity: `file_review`. All 113 assigned files and 34,760 lines were examined: 108 with
full bodies and five declaration-only files. Root checked exact baseline coverage and the complete
three-change fixture diff before accepting [coverage](reviews/syndic-lifecycle.tsv).
[Structured findings](reviews/syndic-lifecycle-findings.json) retain detailed evidence. Their local
`subsystem_review` labels do not complete the remaining Syndic subsystem review.

- SL-001 removes reader forwarding and a duplicate parent-index constructor: 60-70 estimated net
  lines, retaining existing generic bounds, error propagation and read order.
- SL-002 shares pure pending-turn and initial projection constructors: 120-180 estimated net lines.
  Each admission/promotion/continuation path retains its reads, arithmetic, eligibility and effects.
- SL-003 shares two narrow contribution pairs: 80-120 estimated net lines. Binding/head and
  reservation/membership writes stay at their existing positions with option handling local.
- SL-004 shares exact promotion route-successor derivation: 55-85 estimated net lines. Preserve
  unchanged unrelated heads, prior transition and promotion proofs, specific error mapping and
  separate descendant authentication. This owns the route portion previously deferred in SQ-R02.
- SL-005 shares lifecycle fixture construction: 500-580 estimated net lines. The 291-line mixed
  abandonment copies differ only in visibility/borrowing; queued-input, stop and awaiting-terminal
  setup also repeat. Preserve exact records, command/revision counts, IDs and all test scenarios.
- SL-006 shares named/generic abandonment fault driving: 60-80 estimated net lines. Keep both
  seeds, all six fault executions, custody/refusal checks and the stronger persistence assertion.
- SL-R01 retains immutable lifecycle witnesses, checked counters and distinct ordinary/provider
  transitions. General state-machine or serialization dependencies do not replace these contracts.

The six proposals total 875-1,115 estimated net lines. SQ-R02 retains zero savings so SL-004 is
counted once; activity-fold reuse is assessed separately in the remaining projection cohort.
No source, tests, manifests or runtime state changed; no build or test was performed.

## Application Projection Control: AX-001 Through AX-020

Evidence maturity: `file_review`. All 108 assigned files and 28,876 lines were examined: 102 with
full bodies and six declaration-only files after root depth checks. Root checked exact baseline
coverage, both new defect paths and the duplicate fixture size/identical asset helper before
accepting [coverage](reviews/app-projection.tsv).
[Structured findings](reviews/app-projection-findings.json) retain full evidence and verification.
Their local `subsystem_review` labels do not complete the remaining application subsystem review.

- AX-001 shares manual/lifecycle compaction admission tails: 60-90 estimated net lines, preserving
  origin, owned projection and exact cleanup. Correct AX-019's failed admission ownership first.
- AX-002 uses AtomicUsize::fetch_max for high-water updates: 8-12 estimated net lines, retaining
  ordering and separate saturation semantics.
- AX-003 shares native acquisition, retirement classification and decision validation: 135-195
  estimated net lines. Exact revisions, source flights, consuming authorization and cleanup remain.
- AX-004 shares recovery abandonment tails and removes duplicate owned-status validation: 65-105
  estimated net lines. Preserve existing diagnostics and unknown-generation provenance.
- AX-005 represents single-owner projection/work custody directly: 55-85 estimated net lines.
  Required fields replace take-only Options; an unshared worker admission is folded into its permit.
  Preserve field destruction order and the Options that actually support partial ownership transfer.
- AX-007 removes unused provider-frame dispatch/read adapters: 150-185 estimated net lines. This
  private dispatch island is separate from AC-004; actual staging and publication fences remain.
- AX-008 removes constant-false health retry machinery: 20-35 estimated net lines. Keep all
  before/after health checks and failure precedence. Two connection-consumer callers are excluded
  from this estimate; the dead status loop is counted only in AX-007.
- AX-009 shares master-gate election branches: 22-35 estimated net lines, retaining local versus
  persistent-only precedence, exact counts and gate-held callback ordering.
- AX-010 removes unused publication forwarding and inputs: 65-100 estimated net lines. Current
  commands retain actual authority and custody; binding-revision results remain explicit.
- AX-011 shares service shutdown mechanics: 65-100 estimated net lines. Both orchestration paths
  retain their different order, error classification and all-joins behavior; AC-003 is separate.
- AX-012 removes an unwaited Condvar and single-field pending-result wrapper: 15-25 estimated net
  lines. Actual master-gate drain and target-completion waits remain.
- AX-013 removes dead stop error variants/branches and unread local timeout: 15-25 estimated net
  lines. Confirm the public Rust enum change against consumers; retain conservative unresolved claims.
- AX-014 shares two approval barrier release latches: 25-45 estimated net lines. Identity registries,
  arrival signals, poison behavior and controller cleanup stay with each owner.
- AX-015 shares duplicate projection/native-scheduler fixture trees: 1,650-1,760 estimated net
  lines from a 1,779-line base. Preserve scheduler extensions and distinct retirement preconditions;
  real submission, provider frames, bounded builds and all scenarios remain.
- AX-016 shares recovery server and capacity assertion mechanics: 160-230 estimated net lines.
  Independent wire expectations, cancellation cuts, timeout differences and joins remain.
- AX-017 shares strict command-outcome assertions and repeated test setup: 150-230 estimated net
  lines. Clean commit, later failure, indeterminate custody and named race observations stay distinct.
- AX-006 records a zero-savings correctness defect: a readiness scan eagerly takes a commandless
  route's continuation before skipping it. Check command readiness before transferring custody.
- AX-019 records a zero-savings correctness defect: failed durable admission can return after
  local compaction ownership is installed but before cleanup. Release exact local ownership on
  definitive nondispatch failure while retaining indeterminate/later-failure custody.
- AX-018 adds complete boundary-test evidence to AS-009 with zero additional savings. Replace
  positive source-spelling checks only with behavior evidence; retain forbidden-authority checks.
- AX-020 retains partial-construction worker disposition as existing rework, with zero savings.
  Already-started workers still require cancellation and explicit joins on later construction failure.

The sixteen reduction proposals total 2,660-3,257 estimated net lines. Correctness, boundary-test
overlap, existing rework and BR-004 metrics are excluded. Report `correctness_gap`, `overlap` and
`existing_rework_gap` entries are tracking classifications, not implementation authorization.
No source, tests, manifests or runtime state changed; no build, test or GUI launch was performed.

## Syndic Projection Mutations: SX-001 Through SX-R02

Evidence maturity: `file_review`. All 93 assigned files and 24,172 lines were examined: 90 with
full bodies and three declaration-only files. Root checked the exact baseline-bound set and
dependency fit evidence before accepting [coverage](reviews/syndic-projection.tsv).
[Structured findings](reviews/syndic-projection-findings.json) retain full derivations and estimates.

- SX-001 reuses the existing activity effect for child handoff publication: 35-45 estimated net
  lines, preserving handoff admission, exact record families and contribution order.
- SX-002 shares freeze/finalize next-item admission and selected-history effects: 60-90 estimated
  net lines. Content authentication, projection admission, read/error order and invalidation stay local.
- SX-003 shares pure presentation, assistant-phase and activity-order derivations: 45-75 estimated
  net lines. Mutation and scrub still authenticate independently and map their own diagnostics.
  This consumes the activity/phase part of SQ-R02; SL-004 owns the separate route derivation.
- SX-004 shares initial content chunk/span verification through presence-aware loops: 20-30
  estimated net lines. Manifest-first rejection, exact equality and point-read failures remain.
- SX-005 shares paragraph/fallback buffer mechanics: 50-80 estimated net lines. Exact source bytes,
  persisted checkpoint shape, thresholds, markers and distinct fence/table mechanics remain.
- SX-006 couples prepared projection records to publication state: 15-30 estimated net lines.
  Keep independent resource/index presence checks, read order and exact collision validation.
- SX-007 shares provider, recovery, transcript and catalog fixture mechanics: 300-400 estimated net
  lines. Fault cuts, independent expected records, bounded loops and current/historical distinctions
  remain. PRE-004, SP-005, SQ-006 and SL-005 are excluded from this estimate.
- SX-R01 retains the durable bounded parser after a
  [pulldown-cmark 0.13.4 screen](../../memory/crates.io/pulldown-cmark/0.13.4/durable-projection-parser-screen.md).
  Its eager input-wide first pass and private parser state do not replace persisted bounded resume.
  Closed-block integration has no demonstrated substantial net savings; estimate zero.
- SX-R02 retains exact witnesses, bounded traversal and historical distinctions. SQ-005's two
  mutation error mappings are confirmed. PRE-004 owns temporary-home savings, with store-before-home
  destruction required for catalog and recovery-budget fixtures on Windows; estimate zero here.

The seven new proposals total 525-750 estimated net lines. A separate 90-125 mutation-only extension
is included once under SP-003 above. No source, tests, manifests or runtime state changed; no build,
test, installation or GUI launch was performed.

## Syndic Core And Support: SC-001 Through SC-010

Evidence maturity: `file_review`. All 70 assigned files and 23,648 lines were examined: 68 with
full bodies and two declaration-only files, including all five live first-party examples.
Root checked exact coverage, baseline validation and the discarded fixture's pure preparation path
before accepting [coverage](reviews/syndic-core.tsv).
[Structured findings](reviews/syndic-core-findings.json) retain complete alternatives and verification.

- SC-001 specializes legacy prepared content for its sole UTF8 caller: 115-175 estimated net
  lines. Remove unconstructed marker/range machinery while retaining exact chunks, digests, Unicode
  cuts and empty-input semantics. ComposerV1's separate fold remains.
- SC-002 derives core error formatting with existing thiserror: 315-395 estimated net lines.
  Preserve all messages and conversions, including deliberately absent validation error sources
  and attachment Read's source without adding a From implementation.
- SC-003 declares production family registration/reconciliation membership once: 65-80 estimated
  net lines. Preserve order and exact old/new classification. Keep expected-family tests independent;
  the existing 86-versus-90-family authority gap is separate required work with zero savings.
- SC-004 shares 17 pristine-thread required-read mechanics: 125-165 estimated net lines. Keep
  point-read order, distinct absence diagnostics, revision stabilization and all eligibility checks.
- SC-005 shares five example stage-outcome handlers: 75-95 estimated net lines. The live example
  remains; seal custody, reconciliation installation and later-failure distinctions stay explicit.
- SC-006 declares test-only fixture and physical-family dispatch separately: 220-300 estimated net
  lines. Keep literal physical names, composite keys, fault byte offsets and independent production
  oracles. FixtureDelete and required family additions are excluded.
- SC-007 reuses local corruption reconstruction: 150-190 estimated net lines. Every malformed
  variant and coordinated build/receipt/session update remains. This supersedes PRE-007; DT-002
  progress representation and DP-002 staging reconstruction are excluded.
- SC-008 reuses the existing history-record deletion for frontier-only faults: 25-35 estimated
  net lines, retaining the public helper, exact reservation/key and every corruption case.
- SC-009 removes provider fixture construction discarded by both consumers: 780-1,020 estimated
  net lines. The 392-line item-record module and producer-only setup have no durable effects.
  Preserve exact ordered retained records, route facts, real provider seeding and the temporary
  store used to construct retained draft references. SP-005 and PRE-004 are excluded.
- SC-010 shares the semantic fixture lifecycle: 35-50 estimated net lines. Registration and
  recovery still use separate fresh homes, exact diagnostics and independent corruption assertions.

The ten proposals total 1,905-2,505 estimated net lines. No new dependency is needed; thiserror's
existing [investigation](../../memory/crates.io/thiserror/2.0.18/manual-error-simplification.md) applies.
No source, tests, manifests or runtime state changed; no build, test or installation was performed.

## Syndic Draft History And Materialization: DH-001 Through DH-012

Evidence maturity: `file_review`. All 46 assigned files and 15,728 lines were examined: 39 with
full bodies and seven declaration-only files. Root checked exact baseline coverage and the partial
successor predicate against schema authority before accepting [coverage](reviews/syndic-history.tsv).
[Structured findings](reviews/syndic-history-findings.json) retain complete evidence. Their local
`subsystem_review` labels do not substitute for the pending complete-subsystem synthesis.

- DH-001 consolidates the union of local materializer validation: 110-165 estimated net lines.
  Preserve engine-only content identity, ceilings and terminal summary checks, decode/error behavior,
  and separate storage validation. Remove terminal reachability clones only after equivalence checks.
- DH-002 keeps one encoder representation and updates changed build fields directly: 85-130
  estimated net lines. Persisted fields/phases, independently owned admission states and bounds remain.
- DH-003 shares pure history stack/accounting and ancestry mechanics: 90-145 estimated net lines.
  Writer predecessor/root authentication and ordinary read trust remain distinct acquisition paths.
- DH-004 shares historical settlement closure and marker-root mechanics: 40-70 estimated net
  lines. Captured candidate, request bytes, successor closure and mutable-anchor observations remain local.
- DH-005 shares private frontier rekey/recharge mechanics: 40-55 estimated net lines. Keep the
  two named entry points, exact key-size charges, unchanged source and budget boundaries.
- DH-006 borrows prepared successor effects from their settlement: 15-30 estimated net lines.
  Remove three duplicate payloads/clones while retaining four-record atomic contribution and custody.
- DH-007 reuses the existing transition-reference codec: 20-30 estimated net lines, preserving
  exact byte order, complete references, invalid-tag and trailing-byte rejection.
- DH-008 shares ordinary-edit history fixture construction: 200-300 estimated net lines.
  Default-feature support, explicit positions/budgets, fault timing and independent charge oracles
  remain. This owns support changes excluded by DP-007; PRE-004 and other cohort callers are excluded.
- DH-009 reuses local materializer fixture, acceptance and completion helpers: 120-170 estimated
  net lines. Keep all four resource-bound assertions, hard step limits, fault/restart loops and active
  byte oracles. Two unreferenced atom appenders can be removed.
- DH-010 derives three history/materializer errors with existing thiserror: 30-45 estimated net
  lines. Keep exact Display, manual conversions and the currently absent error source chains.
- DH-011 records a zero-savings correctness defect: a noncommitted historical settlement with one
  or two successor fields passes `committed == all_three_present`. Encoding, decoding and status
  rely on that predicate. Require all three fields for commit and none for noncommit. Normal current
  constructors emit all-or-none; the finding concerns static malformed-record acceptance.
- DH-012 retains durable lineage authority and resumable materialization. Fixed ancestry,
  published/newest separation, bounded output and atomic sealing remain required. Prior Ropey/im
  screens apply; the existing family-count authority gap is already tracked under SS-010.

The ten reduction proposals total 750-1,140 estimated net lines. The JSON expresses these as negative
line deltas; this register expresses positive reductions. Correctness and retained entries add zero.
No source, tests, manifests or runtime state changed; no build, test or installation was performed.

## Application Window And Notifications: AW-001 Through AW-007

Evidence maturity: `file_review`. All 41 assigned files and 11,301 lines were examined: 39 with
full bodies and two declaration-only files. Root checked the exact baseline-bound set and both
notice-test evidence gaps before accepting [coverage](reviews/app-window.tsv).
[Structured findings](reviews/app-window-findings.json) include authority and effective rigor.

- AW-001 shares prepared-theme value projection: 45-65 estimated net lines. Keep strict shell
  requirements and explicit widget fallbacks, exact role policy and atomic appearance generations.
- AW-002 shares mounted-window and theme fixture construction: 140-210 estimated net lines.
  Preserve preview attachment, exact claim acquisition, unpublished/published cleanup distinctions,
  real storage/GPUI boundaries and every intermediate lifecycle assertion. PRE-004 is excluded.
- AW-004 shares the visual-row caret fallback: 8-12 estimated net lines. Keep preferred-row
  ambiguity resolution, operation-specific no-layout behavior and grapheme-safe selection.
- AW-003 is an optional product-contract alternative: Copy Detail replaces partial selection,
  saving 330-430 estimated net lines after copy/focus/input adaptation and replacement verification.
  It supersedes PRE-014 and is mutually exclusive with AW-004. Shaping, wrapping, scrolling,
  continuity and stale/inert fences remain. Notification feature and widget authority must change first.
- AW-005 records two zero-savings verification gaps. The append-continuity fixture already exceeds
  the detail cap, so appending leaves projected text unchanged. Test command selectors use the
  replacement counter while rendering uses interaction generation, which also advances on inert
  and appearance changes. Repair the evidence without changing actual interaction fences.
- AW-006 retains explicit creation and shell custody, reconciliation and readiness checks.
  Existing missing mounts are paused rework; fixture destruction order belongs under PRE-004.
- AW-007 retains the bounded notice arbiter and exact mounted routes. Its 16-entry policy queue
  and one bounded visible detail do not justify a generalized queue or event framework replacement.

The three behavior-preserving proposals total 193-287 estimated net lines: 53-77 source and
140-210 tests. If the Operator adopts Copy Detail with revised authority, replacing AW-004, the combined total
would be 515-705; these totals are alternatives, not additive. No new dependency was justified.
No source, tests, manifests or runtime state changed; no build, test, GUI launch or installation
was performed.

# Subsystem And Whole-Codebase Synthesis

Complete file coverage was verified on 2026-09-07: all 1,910 files and 578,865 lines are accepted,
with 1,847 full-body reviews and 63 declaration-only reviews. The exact tracked candidate set has
2,251 paths: 1,910 included plus 341 explicit exclusions. There are no missing, duplicate,
nonaudited, invalid-depth or empty-evidence rows; the final merge checked baseline identities and
unchanged live source. Subsystem and whole-codebase synthesis subsequently completed; the final
24-report coverage check required zero ledger changes. All 246 subsystem estimate rows match their
structured decisions. The canonical ledger merges the duplicate PRE-004 share, resolves all 16
preliminary leads and contains 250 unique decisions.

Evidence maturity is now `whole_codebase_review` for the canonical decisions. `Proposed` means a
reviewed recommendation; it does not mean implemented or approved by the Operator. Historical
structured reports use additional local labels such as `accepted`, `merged` and `contract_gated`;
the canonical ledger normalizes these to disposition plus bucket without changing their meaning.

## Final Amounts And Superseding Decisions

- App: 7,345–10,286 current-contract net lines; [synthesis](reviews/subsystem-app.json).
- Foundations: 5,718–8,805; [synthesis](reviews/subsystem-foundations.json).
- Syndic: 7,862–11,383; [synthesis](reviews/subsystem-syndic.json).
- Combined: 20,925–30,474. Excluding BJ-003's prototype-dependent 1,660–2,460 gives
  19,265–28,014. Correctness corrections and existing rework have no savings credit.

The complete rationale and replacement costs are in the final report and structured decisions.
The following revisions supersede the earlier numbers in this historical register:

- AC-006 is 25–35; AI-007 is 100–120; AI-009 is 30–50 with worker/flight ownership excluded
  from the common execution payload. AX-008 is 30–50 including both live consumer adaptations.
- AX-015 is 1,640–1,725 after pricing nine mounts and distinct retirement setup. AI-013's
  separate unmounted modules remain 415. AS-009 includes AX-018's evidence at no extra credit.
- BJ-003 is 1,660–2,460 after excluding all BJ-004 corrective deletion. HS-004 is zero;
  ST-004 is 570–970 after excluding the false concurrency/digest evidence correction.
- DT-003 retains 60–90 only with each root finalizer's normalization/read-error order preserved.
  DT-004's frontier-only schema alternative is 60–85; active-marker removal is retained.
- SP-003 combines its two cohort entries into 180–255 once. SQ-R02's narrow derivations are
  allocated to SL-004 and SX-003. SC-007 includes PRE-007; DP-007 excludes DH-008's common helper.

New synthesis findings are APP-SYN-001 (remaining App temporary-home helper, 12–18), APP-SYN-002
(directory/store lifetime correction, zero), APP-SYN-003 (three forwarding payload gates, 25–40),
FND-SYN-001 (three foundation fixture lifetimes, zero), FND-SYN-002 (concurrency/digest evidence,
zero), SYN-SYN-001 (two omitted ordinary-edit callers, 20–40) and SYN-SYN-002 (Syndic test homes,
260–330). Their complete scope and integration costs are in their subsystem reports.

PRE-001 through PRE-016 all resolve at zero additional savings in the canonical ledger. In
particular, PRE-004 combines App and Syndic shares once; foundations already use tempfile.
PRE-015 remains an unmeasured editor/history contract alternative. PRE-016 belongs to existing
tree/service ownership rework. Copy Detail, capped TOML and new value formats are alternatives
with explicit overlap, not additions to all current local work.

## Final Review Limits

No product source, tests, manifests, design authority or implementation plan changed. No product
builds, tests, GUI runs or dependency installation occurred. Coverage validation, static relationship
checks, dependency evidence and accounting support this audit. Implementation still requires its
focused verification and the owning consequential-boundary review. A fully audited baseline is
neither proof of irreducibility nor certification that the unfinished baseline application runs.
