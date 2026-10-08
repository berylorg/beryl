# Runtime and session state

This supplement is governed by [the `beryl-state` design](design.md) and normatively owns only
runtime/root, session/window, and thread-claim durable state.

## Runtime and root records

- Exact runtime-record reads are also available through explicit candidate recovery access for
  service preparation. They retain ordinary point-read limits, decoding, exact home/generation
  qualification and confirmation, without opening ordinary admission or publishing the candidate.
- A runtime records its stable id, canonical absolute Codex App Server launch executable identity, derived exact
  Host or WSL-distribution mode, runtime-native executable path, environment label, creation facts,
  explicit standalone-server or CLI launch form, availability summary, and nonzero monotonic record revision. The runtime codec requires a valid closed launch-form tag; missing or unknown tags are rejected rather than defaulted. A root records its stable id,
  runtime id, canonical runtime-native path, full path, non-removable fact, availability, activity,
  and revision.
- One canonical executable identity has at most one runtime. Canonically equivalent roots under one
  runtime have one root record. A root's canonical filesystem environment must equal its runtime's
  derived mode. Runtime creation and its non-removable user-home root are one validated atomic
  contribution; runtime and root records are additive only.
- Runtime/root ids, paths, modes, availability observations, and times are admitted caller facts:
  this package performs no filesystem, clock, WSL, process, or CAS observation. Availability is the
  bounded shared category plus optional Unix-millisecond observation time; unknown has none. Root
  activity changes only to a strictly later optional caller-supplied Unix-millisecond maximum.
- Every mutation requires the exact record, home, and domain revisions. A catalog join reads one
  exact runtime/root pair and validates its records and root-id index again in the serialized
  command before another domain publishes its projection.
- `CreateRuntimeWithHomeRoot::initial_catalog_source` exposes the exact future initial records
  from that validated creation contribution, for a first-runtime catalog join in the same command.
  They are planned facts rather than a persisted-source proof and must accompany runtime/home
  creation. `PreparedWindowClaimReplacement::catalog_claim` derives the future catalog claim from
  its opaque Session replacement; the app joins it with that exact replacement contribution.

## Session and windows

`SessionState::prepare_window_claim_replacement` authenticates the bounded Running header, exact
invoking window, paired predecessor and target occupancy. It returns current selection, an exact
claim elsewhere, or an opaque replacement prepared through the ordinary replacement rules.
State derives the future window, claim, header and revisions. Its contribution revalidates the
original Session-domain revision and exact source records, permitting unrelated Home-domain
changes from unpublished composer preparation and ordinary draft flush. Its outcome classification
distinguishes the exact original, exact committed replacement and collision; no result alone grants
execution dispatch, view publication or replay. Elsewhere occupancy also authenticates the
target's referenced window and remembered runtime/root binding.

Exact original/replacement classification is also available through explicit same-home candidate
recovery access using the retained original prepared replacement evidence. It authenticates the
complete window and both paired claim copies with bounded named reads, including remembered
runtime/root and relevant revisions. Missing, mixed, foreign or conflicting facts are collision,
not evidence of noncommit or replay authority. Candidate classification creates no claim command
and does not replace original HomeCommand outcome reconciliation.

- The `beryl-session` domain owns one active header, restorable main-window records, and claims
  keyed both by window and by Syndic thread. One home has at most 256 restorable main windows. The
  V1 header is fixed-capacity; each V1 window record is fixed-size with canonical tagged padding
  for optional identities and monitor facts. An all-zero identity is valid, never an absence marker.
- The header stores generation, orderly-Exit intent, sorted unique restore-set `(window id,
  expected window revision)` references, and the last successful runtime/root fallback when the set
  is empty. A window stores its id, selected Syndic thread, remembered runtime/root, placement and
  size, supported virtual-desktop identity, and revision. Auxiliary windows, flyouts, menus,
  notices, and previews have no session records.
- The only valid threadless record is the sole zero-runtime initial window: it has no remembered
  target, selected thread, claim, or fallback. Minimal discovery reads only the fixed header and
  referenced windows, rereads the header, and rejects concurrent publication; it does not load
  catalog, transcript, CAS, or draft state.
- Threadless initialization distinguishes an absent header from an exact observed empty header.
  An existing header must match the supplied session revision, contain no window references and
  carry no runtime/root fallback; success advances its session revision and installs one fresh
  threadless window with Running exit intent. An absent-header request cannot initialize over an
  existing header, and an exact-header request cannot recreate a missing one. Both paths reject
  pre-existing records for the chosen window identity and preserve ordinary command outcome and
  reconciliation semantics. Zero-runtime eligibility remains the composing startup owner's
  responsibility; an empty session alone does not prove an empty runtime registry.

- Dedicated Exit may publish all current window placements and the orderly-Exit marker in one
  typed session contribution. Its input contains 1–256 unique window identities, each exact
  expected window revision and admitted placement, under an exact session and domain revision.
  The writer requires that input membership equal the entire active header; omitted, extra,
  duplicate, stale or already-exiting input publishes nothing. Input order has no meaning.
- That publication advances the session revision once and every included window revision once,
  including unchanged placements. It preserves selected threads, remembered targets, paired
  claims and fallback. It reserves the complete header/window reconciliation
  closure and uses the existing HomeCommand durability and ambiguous-outcome protocol. The app
  must prove work and draft readiness, capture native facts and settle the command before final
  teardown; this state mutation alone grants no shutdown, claim release or native destruction.

## Resume A Committed Exit Session

- `ResumeSessionAfterExit` contributes one header-only transition from OrderlyExit to Running.
  Its input supplies the exact session revision and 1–256 unique `(window id, record revision)`
  pairs; order is irrelevant. Empty, oversized or duplicate input is rejected. Preparation requires
  an existing OrderlyExit header and exact equality with its complete window reference set.
- The enclosing HomeCommand retains exact home and domain revision checks. Success advances the
  session revision once and changes only its exit intent. Window records and revisions, placements,
  selected threads, remembered targets, paired claims and their revisions, and fallback remain
  unchanged. Missing, stale, incomplete, foreign or already-Running input publishes nothing.
- Reconciliation reserves only the changed header and retains ordinary noncommit, commit with
  later failure, and indeterminate outcomes. This contribution performs no service replacement,
  claim restoration or window operation. The composing recovery owner must prove that the original
  Exit failed, preserve known outcomes and validate the exact source under fresh authority before
  using it; the contribution alone grants no reopening, interaction release or shutdown authority.
- Verify threadless and claimed sets through capacity, identity and claim preservation after
  reopening, rejected malformed or stale inputs, writer revision drift, and ambiguous and
  postcommit outcomes. Independently review this persistence transition.

## Recover A Removed Live Window

- State captures one opaque immutable removal evidence value from a coherent healthy-home read
  of the exact Running header, one referenced window and its matching active paired claim, or
  the sole valid threadless window. Bracket these reads with equal home revisions. Retain only
  the durable home id, canonical home path and original process home-generation identity,
  fixed-capacity header, one window and optional claim;
  no store, domain handle, lease or receipt is retained. Missing, changed, malformed or restoring sources
  refuse capture. Before admitting removal, check availability of both session successors and the
  one window and optional claim successor needed by restoration. Home and domain revision
  exhaustion retains the ordinary command refusal and unavailable-recovery semantics; capture
  reserves no future writer revision.
- Evidence supplies the exact ordinary removal contribution under the caller's current domain
  revision. The writer validates the complete original header, exact window and claim before
  removal, so evidence cannot describe a different successful removal. Existing ordinary command
  noncommit, commit-with-later-failure and reconciliation custody remain the caller's responsibility.
- A separate recovery contribution accepts that evidence through fresh same-configured-home
  candidate authority or authenticated ordinary access to the original still-Healthy home
  generation. Both routes use the same exact source/result validation and mutation algorithm;
  ordinary access does not bypass home admission or grant candidate authority.
  It requires the exact post-removal Running header: original revision plus
  one, original sorted member set minus only that window, unchanged fallback. The removed window
  record and both relevant claim indexes must be absent. Every mismatch, exhausted revision,
  conflicting claim or foreign home refuses without writes. The caller must separately prove the
  original removal committed and its close was cancelled; evidence alone proves neither.
- Success advances the session revision once, restores the same window with its original record
  revision plus one, and restores both claim copies with their original claim revision plus one.
  Claim generation, Active state, thread identity, remembered target, placement and fallback are
  preserved. The restored window selection references the renewed claim revision. Other windows
  and claims remain unchanged. Threadless recovery installs no claim and only accepts an empty
  post-removal header with no fallback. The existing 256-window capacity remains binding.
- Candidate validation distinguishes the exact original state, exact removed state and exact
  recovered state using this evidence, including the paired claim copies; it never classifies
  absence without header and identity proof. Exact surviving header references and writes confined
  to the affected member preserve otherwise valid records; this adds no recovery-time domain scan.
  Recovery reserves only its changed
  header, window and optional paired claim records for ordinary HomeCommand reconciliation.
  Duplicate recovery cannot apply twice. No durable journal, tombstone, startup restoration or
  new on-disk schema is introduced.
- Ordinary healthy access exposes the same bounded classification for the original generation.
  Removal and restoration have separate command outcomes and reconciliation custody. An unrelated
  home writer revision may require fresh preparation, but cannot relax exact session/window/claim
  comparisons. The composing caller retains cancellation and exact native-survival proof, either
  settled native survival or the app's separately qualified custody before destruction admission; State evidence
  alone authorizes neither reopening nor native retry. If the home fails, ordinary admission closes
  and retained immutable evidence continues through the existing candidate recovery route.
- Verify claimed and threadless members, final and nonfinal sets through capacity, preserved
  identities/placement/fallback, monotonic revisions, stale/foreign/conflicting source refusal,
  duplicate recovery, writer revision drift, candidate reopening and ambiguous/postcommit outcomes.
  Exercise healthy-generation restoration and classification with equivalent refusal and outcome
  coverage, including transition to failed-home candidate recovery without a duplicate write.
  Independently review this persistence transition. App-owned cancellation, original outcome
  classification, service replacement and interaction release remain separate obligations.

## Reverse thread claims

- Window-keyed claim observation is also available through explicit candidate recovery access.
  It shares ordinary identity and reverse-copy validation and fixed point-read limits, requires
  fresh typed home/generation authority and propagates confirmation failures. It writes nothing,
  grants no ordinary admission and does not substitute for window or whole-session qualification.
- Exact claim sources may be read by window or thread identity. A present source validates its
  requested identity and matching reverse copy; an absent source proves absence at that exact
  key without scanning the claim collection. Whole-home validation and atomic paired mutations
  retain responsibility for complete reverse-index consistency.
- A claim records its exact window id and Syndic thread id in both reverse forms, publishing
  session revision, active-or-restoring state, and a separate monotonic claim revision. One window
  has at most one active/restoring claim and one thread has at most one active/restoring window.
  Claim, release, restore, and claim-or-create validate both reverse indexes in the serialized
  command; missing or disagreeing copies are corruption.
- Stale asynchronous observations cannot clear or replace a newer claim generation. Paired claims
  outside the active header/window set are only bounded stale startup state and are removed by the
  revision-checked begin-restore command. Claims are crash/session coordination facts, not OS locks
  and not a substitute for the one-process home lock.
- A catalog join uses one bounded claim point read proving matching reverse copies or thread-keyed
  absence, then carries that present-or-absent fact as an exact validation-only participant.

## Prepublication window abandonment

- The session boundary prepares one bounded exact-window abandonment candidate only for a claimed
  window that the caller identifies as acquired but not yet published visible. The candidate binds
  the current session revision, header generation, exact restore-set reference, window record, and
  both reverse claims to the caller's `WindowId` and private acquisition fingerprint.
- Its mutation contribution removes the exact restore-set reference, window record, and both
  reverse claims atomically and advances the session revision. A missing, changed, rebound,
  partially present, or disagreeing member rejects without deleting any session or claim record.
- Its natural-state read uses bounded point reads and classifies the session-owned closure only as
  exact acquired, exact abandoned, or collision. It does not inspect Syndic records, durable jobs,
  catalog rows, visibility, or application custody and cannot authorize a retry by itself.
