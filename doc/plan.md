# Scope

Execute only Checkpoint 2 of the Beryl-home architectural rework tracked by `doc/rework/beryl-home/REWORK.md`: create the permanent Beryl-home physical-store and Beryl-owned state packages, reconstruct the shared pure identities they require, and implement the accepted locking, typed-domain, durability, health, recovery, registry, and minimal session-read foundations.

Checkpoint 1 is independently verified complete. No Checkpoint 2 code may import archived source, read workspace-era state, expose raw Fjall handles or encodings outside `beryl-home-store`, open a second Syndic database, or introduce a compatibility adapter. New packages and source live only at their final target paths.

This checkpoint does not implement Syndic thread/draft records, CAS execution projections or `thread/inject_items`, runtime executable admission or backend launch, claim-or-create thread workflows, main-window or startup-failure GUI, full catalog joins, image byte admission/runtime projection, branch handoff, semantic search, or theme-editor redesign. Those remain assigned to later rework checkpoints. The `beryl` process entry may retain its explicit compile-time bootstrap gap; Checkpoint 2 must not replace it with a runnable placeholder before the target app startup surfaces exist.

# Phase 1: Prove Storage And Windows Ownership Primitives (finished)

Operator resolution on 2026-07-13: use the exact current official Fjall 3.1.6 release despite its known batch journal-write error-suppression defect. The workspace dependency is pinned to `=3.1.6`; establish and verify its actual `Cargo.lock` entry when the permanent `beryl-home-store` package begins consuming it in Phase 2 rather than creating a temporary consumer.

Accepted dependency exception: Fjall 3.1.6 retains the same discarded `journal_writer.write_batch(...)` result as 3.1.5. The defect is documented in `crates/beryl-home-store/doc/design.md` and reported upstream as `fjall-rs/fjall#304`. It no longer blocks Checkpoint 2, but implementation must use the ordinary official API, retain the explicit `SyncAll` barrier and fail-closed handling for surfaced errors, add no workaround or adapter, and make no false claim that the suppressed-error path has been proved safe. A corrected release or owned fork remains a later explicit decision.

- Inspect the exact Fjall 3.1.6 and Windows 0.61 APIs needed for multi-keyspace atomic batches, `SyncAll`, bounded reads, close/reopen behavior, non-blocking file locking, opened-object canonical identity, UNC capability checks, and file/directory durability.
- Record reusable dependency evidence under `doc/memory` according to the exploration-memory and Cargo rules, including any API limitation that constrains the target design.
- Prove a permanent testability strategy for commit, persistence-barrier, sidecar, and reopen faults plus subprocess crash cut points; do not introduce a production abstraction that weakens or disguises Fjall behavior.

Edge cases include journal commit succeeding before a persistence barrier fails, lock-file existence without a live owner, case and extended-path aliases, symlinks, junctions, unsupported or unreliable UNC semantics, a directory disappearing between admission steps, and an engine API that cannot atomically span registered domains.

Verification used exact dependency artifacts and focused disposable probes and identified the required Cargo features. The Fjall suppressed-error path remains the explicit Operator-accepted exception; every other contract must still be implemented without an adapter or implicit weakening.

Resumable milestone achieved: the physical storage, locking, durability, and fault-test primitives are understood against exact dependency APIs, the upstream limitation and accepted exception are explicit, and Phase 2 may establish the permanent package graph.

# Phase 2: Establish Target Packages And Pure Shared Values (finished)

- Add `beryl-home-store` and `beryl-state` as final Cargo workspace packages with their accepted dependency direction and package documentation intact; pin and verify the exact Operator-approved Fjall artifact because fail-closed recovery currently also depends on exact-version, doc-hidden APIs.
- Reconstruct `beryl-model` with opaque home, window, runtime, root, Syndic-thread, revision, command, idempotency, availability, execution-binding, placement, and provenance values required by this checkpoint.
- Keep identity generation and filesystem observation out of `beryl-model`; expose only bounded construction, parsing, validation, ordering, and serialization where a consuming boundary requires it.

Implementation result: `beryl-home-store` and `beryl-state` now exist as permanent workspace packages. `beryl-home-store` depends on `beryl-model` and is the only new package that consumes Fjall; `beryl-state` depends on those two lower boundaries. `beryl-model` now exposes distinct bounded stable identities, monotonic scope-specific revisions, admitted Host/WSL path values, immutable execution bindings, categorized availability, restorable placement facts, and bounded provenance without generating identities or observing the filesystem.

Edge cases include malformed or oversized identities, empty WSL distribution names, Host/WSL mode confusion, path values that are syntactically present but not yet admitted, revision-type mixing, and serde shapes accidentally becoming storage-schema authority.

Verification must pass focused model tests, Cargo metadata and package-policy checks, documentation and formatting checks, and dependency scans proving the pure model has no GUI, Fjall, async runtime, filesystem, process, or backend dependency.

Verification result: the exact lockfile artifact is Fjall 3.1.6 with checksum `9fcdc69609906151dff9b534e30eaf8515082055d36f628e382bd0b5d6a1d362`, exact `lsm-tree` 3.1.6, and only Fjall's default `lz4` feature. Focused nextest, Clippy with warnings denied, Cargo documentation, locked metadata, formatting, source-size, dependency-tree, and forbidden-dependency scans pass; `beryl-model` has only the approved Serde runtime dependency.

Resumable milestone achieved: the new package graph resolves and the remaining storage work can use final pure identities without reviving an obsolete model. Phase 3 may implement canonical home opening and exclusive ownership without beginning typed domain registration.

# Phase 3: Implement Canonical Home Opening And Exclusive Ownership (finished)

- Implement the `beryl-home-store` open boundary, fixed home lock file, non-blocking exclusive OS lock, retained ownership handle, canonical opened-directory identity, physical layout admission, home schema header, and distinct busy, unsupported-lock, unsupported-schema, unreadable, and open failures.
- Persist or validate one opaque home identity only after ownership is established; never infer identity from user-facing path spelling alone.
- Return typed busy/unreadable outcomes for later process/UI composition without starting CAS, reading session state prematurely, or creating a runnable shell placeholder.

Implementation result: `beryl-home-store` now owns the complete physical open boundary. It opens and identifies the actual directory object, rejects remote homes and reserved-layout collisions, retains the directory and fixed `home.lock` handles for the store lifetime, acquires one non-blocking exclusive byte-range lock, distinguishes busy, unsupported-capability, schema, unreadable, and ordinary open failures, and creates or validates the fixed Beryl-home header without exposing Fjall or Windows handles. Existing nonempty or malformed state is recovered or rejected in place and is never replaced by a fresh database.

Edge cases include concurrent processes, process death leaving the lock file, explicit orderly release, aliases reaching the same opened file, nested or colliding database paths, missing parents, read-only directories, removable storage loss, and UNC shares that cannot prove required locking and durability.

Verification must include subprocess ownership races, stale lock reuse, case/extended-path/symlink/junction identity checks, fail-closed rejection of generic UNC and mapped remote homes, schema-version rejection, and proof that failure never opens or replaces an empty database over unreadable state. No remote home is admitted unless a later provider-specific contract can prove persistent lock ownership and stable-storage flush behavior.

Resolved verification constraint on 2026-07-13: ordinary execution could not create the required real directory symlink because Windows returned error 1314 (`ERROR_PRIVILEGE_NOT_HELD`), but the Operator confirmed inline elevation is available. The unchanged alias suite passes under `sudo`, including real symlink, real junction, case, and extended-path aliases; no case was skipped or replaced.

Verification result: the elevated focused suite passes all 23 tests across physical opening, alias identity, and subprocess ownership. It covers live contention, orderly and forced-process release, stale lock reuse, retained-handle replacement denial, case and extended spellings, real directory symlinks and junctions, reserved-state junction rejection, schema and malformed-state preservation, and non-destructive open failures. A separate elevated `pushd` proof against `\\localhost\C$` passes the mapped-SMB rejection fixture. Locked check, warnings-denied Clippy, formatting, Cargo documentation, metadata and dependency-tree inspection, public raw-handle/Fjall leak scans, obsolete-source scans, and whitespace checks also pass.

Resumable milestone achieved: one process can acquire one canonical Beryl home and obtain a typed healthy handle, while all ownership/open failures remain explicit and non-destructive. Phase 4 may implement typed domain registration without reopening the physical ownership design.

# Phase 4: Implement Typed Domain Registration And Serialized Commands (finished)

- Implement private logical-domain registration, versioned keyspace families, opaque domain handles, bounded typed point/cursor reads, validators, and sealed mutation contributors inside `beryl-home-store`.
- Implement one process-wide serialized writer that validates expected home/domain revisions immediately before assembling one cross-domain Fjall batch and completes `PersistMode::SyncAll` before returning durable success.
- Exercise the generic boundary with package-private fixture domains only; do not begin Syndic record implementation or expose raw database, keyspace, batch, transaction, codec, or writer-guard types.

Edge cases include duplicate domain/keyspace registration, unsupported record versions, missing required keyspaces, stale expected revisions, contributor validation failure after another contributor succeeds, reentrant writer use, oversized cursor results, and caller cancellation before or after command admission.

Verification must prove all-or-nothing multi-domain commits, deterministic conflict results, explicit read bounds, no raw Fjall type in public APIs, `SyncAll` before success, and concurrent reads without concurrent mutation assembly.

Implementation result: `beryl-home-store` now owns an exact private logical-domain registry, versioned keyspace families and records, opaque domain handles, bounded typed snapshot reads, reopen validators, sealed mutation contributors, persistent home/domain revisions, and one process-wide serialized cross-domain writer. The writer rejects stale revisions and same-store reentry before mutation, observes cancellation only before admission, validates and assembles every contributor before one Fjall batch, and completes `PersistMode::SyncAll` before constructing a durable receipt. Phase 4 exercises the boundary only through package-private fixture domains; no Syndic or Beryl-state product domain has begun.

Verification result: the elevated package suite passes all 42 tests across eight binaries, including atomic two-domain reopen, later-contributor failure with no commit, deterministic conflict ordering, explicit point/cursor bounds and direction, exact schema and record-version rejection, missing control/domain keyspaces, writer serialization with concurrent reads, pre-admission cancellation, post-admission completion, reentry rejection, and immediate process abort after durable success. Locked check, warnings-denied Clippy, formatting, warnings-denied Cargo documentation, metadata, dependency-tree inspection, source-size checks, public raw-Fjall exposure scans, obsolete-source scans, and whitespace checks pass. The Operator-accepted Fjall issue #304 remains an explicit uncovered dependency defect rather than a claimed proof.

Resumable milestone achieved: final typed domains can register and participate in revision-checked durable commands without learning physical storage details. Phase 5 may implement health gating, same-home reopen validation, and sidecar durability without reopening the typed-domain or writer design.

# Phase 5: Implement Health Gating, Reopen Validation, And Sidecar Durability (finished)

- Implement coherent `opening`, `healthy`, `verifying`, `failed`, and `reopening` health states; gate affected reads and mutations after structural or persistence failure; and expose one same-home single-flight recovery boundary with the accepted retry schedule.
- Reopen only the same locked home, reacquire registered domains, run authoritative validators, confirm a persistence barrier, and publish a new healthy generation only after validation succeeds.
- Implement permanent sidecar admission helpers with bounded temporary writes, content flush, atomic final rename, supported directory durability, and metadata-commit ordering; never delete orphaned or unreferenced bytes.

Edge cases include failure before batch commit, during commit, after commit but before `SyncAll`, repeated failure signals during one recovery, reopen failure after engine close, validator disagreement, missing referenced sidecars, failed rename or directory flush, and late completion from an obsolete home generation.

Verification must use permanent deterministic fault seams and subprocess cut points to prove that every failure surfaced through the package or Fjall is never reported durable, gated operations cannot publish new state, recovery never initializes a replacement database or reads CAS, and caller-owned coherent in-memory values remain untouched. The suppressed Fjall journal-write result tracked by upstream issue #304 remains an explicit known gap and must not be represented as covered by these proofs.

Implementation result: `beryl-home-store` now owns one generation-aware admission gate across typed reads, serialized commands, domain registration and reacquisition, and sidecar operations. Surfaced persistence failures enter bounded verification; structural disagreement fails closed; same-home recovery is single-flight, drains the failed Fjall generation while retaining the outer home lock, rejects absent or replacement state, reacquires registered domains, runs ordinary and sidecar-aware reopen validators, completes `SyncAll`, and publishes a new generation only after all checks agree. Obsolete domain handles, queued commands, and sidecar tokens cannot authorize work after recovery. The package exposes the accepted recovery schedule without taking application-window or retry-loop ownership.

Sidecar implementation result: bounded bytes are addressed by namespace, SHA-256, and exact length under one home-wide sharded directory. Admission creates and flushes a unique temporary file, performs a no-replacement atomic rename, flushes the containing directory through the Windows durability primitive, reopens and verifies exact content, and returns a generation-bound retained token that a first metadata-reference command holds through its own `SyncAll`. Existing content is verified before reuse; failed operations retain inert temporary or unreferenced final bytes; no deletion API exists.

Verification result: all 62 elevated `beryl-home-store` tests pass across twelve binaries. The suite covers surfaced commit and persistence faults, bounded verification, repeated and single-flight reopen, failure before and after candidate reopen, retained outer-lock contention, validator disagreement, missing referenced sidecars, database removal without replacement creation, stale handle/command/token rejection, sidecar write, file-flush, rename, directory-flush and verification cuts, exact deduplication, and subprocess aborts before commit, after commit before `SyncAll`, and after `SyncAll` with only coherent old-or-new batches visible after reopen. Locked production and all-feature checks, warnings-denied Clippy, warnings-denied Cargo documentation, formatting, metadata and dependency inspection, source-size, public-Fjall exposure, obsolete-source, and whitespace scans pass. Fjall supplies no injectable boundary inside its private journal write, so upstream issue #304 remains the explicit Operator-accepted unproved dependency gap rather than being counted as covered.

Resumable milestone achieved: the physical boundary fails closed for every surfaced error, preserves exact last-durable authority, and can validate and reopen only the same retained home without fallback data. Phase 6 may register the first permanent Beryl-owned product domains through this boundary.

# Phase 6: Implement Runtime, Root, And Thread-Metadata Domains (finished)

- Register `beryl-state` runtime, configured-root, and thread-metadata keyspace families through the opaque home-store boundary.
- Implement versioned records, bounded queries, uniqueness indexes, revisions, and additive commands for canonical executable runtimes and roots, including atomic runtime-plus-non-removable-home-root creation from already admitted canonical facts.
- Implement immutable thread execution bindings and only accepted automatic metadata transitions: generated title, branch-discussion archive state, activity summary, and token-usage presentation snapshot. Expose no manual rename, pin, archive, delete, removal, or rebind command.

Edge cases include duplicate canonical executables, multiple executables in one environment, canonically equivalent roots, cross-environment roots, missing default home root, unavailable-but-retained records, stale revisions, immutable binding changes, and CAS names or catalog rows entering metadata.

Verification must prove atomic default-root creation, uniqueness under concurrent commands, additive-only registries, immutable execution binding, bounded unavailable facts, strict record-version failures, and no backend, CAS, GPUI, or Syndic-private dependency.

Implementation result: `beryl-state` now registers separate exact-schema runtime/root and thread-metadata domains through `beryl-home-store`. Runtime creation atomically publishes its executable identity and mandatory non-removable home root; canonical executable, root-id, root-path, and home-root indexes are reopen-validated; runtime and root records remain additive while observed availability and root activity advance under exact record revisions. Thread metadata keeps one immutable execution binding and exposes only generated-title acceptance, source-revision-ordered activity and token snapshots, and the one-way successful-handoff archive transition. Every clock, path, environment, availability, identity, and source-revision fact remains caller supplied.

Verification result: all 16 focused `beryl-state` nextest cases pass. They cover atomic creation and reopen, concurrent and direct executable uniqueness, same-runtime root uniqueness with equivalent paths still permitted across distinct runtimes, Host/WSL boundary rejection, missing-home-root reopen failure, retained unavailable records, stale record and source revisions, bounded runtime/root/metadata cursors and values, immutable thread binding, generated-title immutability, one-way branch archival, exact metadata persistence, and unsupported record-version rejection. The 62-case all-feature `beryl-home-store` foundation suite also remains green after the shared constructor lint correction. Locked all-target and all-feature checks, warnings-denied Clippy and Cargo documentation, formatting, workspace metadata, source-size, direct dependency, forbidden-import, and whitespace inspections pass. The package directly depends only on `beryl-home-store` and `beryl-model`; it imports no backend, CAS, GPUI, Fjall, Syndic-storage, archived, or obsolete boundary.

Resumable milestone achieved: admitted runtime/root facts and Beryl-owned thread presentation metadata persist through final typed target APIs. Phase 7 may add session/window, claim, and settings domains without reopening these schemas or exposing storage encodings.

# Phase 7: Implement Settings, Jobs, Catalog, And Asset-Reference Domains (finished)

- Implement versioned typed settings records and atomic multi-setting Apply contributions without redefining feature-owned validation semantics or storing backend-owned Codex state.
- Implement typed durable-job records with exact kind-specific identities, attempts, idempotency, bounded evidence, revisioned state machines, and no generic payload escape hatch.
- Implement compact rebuildable catalog rows, deterministic recency indexes, source revisions, stale markers, bounded exact streaming, and bounded lexical-normalization fields without loading turn bodies, drafts, Markdown, resources, or CAS metadata.
- Implement asset metadata and typed owner-reference records only; keep bytes in home-store sidecars, never store them in Fjall, and expose no byte deletion before future garbage collection.

Implementation result: `beryl-state` now owns four additional exact-schema domains. Settings use a closed typed V1 key set and one revision-checked all-or-nothing Apply contribution. Durable jobs use kind-specific identities, request idempotency, attempt indexes, bounded resolution or failure evidence, and a monotonic checkpoint-aware lifecycle. Catalog rows retain only bounded presentation, execution, availability, claim, lineage, archive, normalized-search, source-revision, and recency facts behind a deterministic recent-first index. Asset metadata uses versioned SHA-256-plus-length identities, typed durable owners, exact reference counts and reverse indexes, and a first-reference contribution inseparable from its flushed sidecar token; final reference removal retains both metadata and bytes.

Edge cases include atomic settings validation failure, unknown setting schemas, regressing terminal jobs, reused attempt identities, oversized failure evidence, equal catalog recency, stale projection publication, byte-bound overflow, digest/length disagreement, missing sidecars, duplicate references, and final-reference removal.

Verification must cover per-domain codecs and reopen validators, revision conflicts, settings all-or-nothing behavior, job state-machine/idempotency rules, deterministic catalog order and bounds, catalog staleness/rebuild admission, typed asset ownership, and absence of durable-byte deletion.

Verification result: all 42 `beryl-state` and 16 `beryl-model` tests pass. They cover the closed settings shapes and atomic rejection, unknown setting and record schemas, job admission/idempotency/attempts/terminal monotonicity/checkpoint-aware failures and evidence bounds, catalog precedence/order/staleness/source revisions/stored-byte bounds/reopen disagreement, asset identity and byte bounds, first-reference revision and sidecar coupling, duplicate owners, missing-sidecar reopen rejection, and zero-reference byte retention. The elevated 63-case all-feature `beryl-home-store` suite remains green and now proves that ordinary registration of an existing domain runs its sidecar-aware reopen validator before handle publication. Locked scoped all-target checks, warnings-denied Clippy and Rustdoc, formatting, metadata, dependency, source-size, forbidden-import, raw-storage-exposure, durable-byte-deletion, and whitespace audits pass. The full workspace all-target command remains at the already-recorded Checkpoint 1 `beryl-app` retained-test cutover boundary; no obsolete theme API was restored to bypass it.

Resumable milestone: every Checkpoint 2 Beryl-owned non-session metadata family is durable, bounded, revisioned, and physically isolated behind the home-store boundary.

# Phase 8: Implement Session, Window, Claim, And Minimal Bootstrap Records (finished)

- Implement the active restore-set header, durable main-window records, last-successful runtime/root fallback, exact placement and virtual-desktop facts, and exclusive thread-claim records with reverse-index validation.
- Implement revision-checked storage commands for window creation/update/removal, orderly-Exit intent, restoring claims, claim release, and atomic session-generation publication, while leaving user-facing close/Exit and cross-domain claim-or-create orchestration to later checkpoints.
- Implement minimal pre-window reads limited to the schema header, active session header, and exact referenced fixed-size window records, returning the exact selected-thread identities and bounds that Checkpoint 3 will use for its current-draft reads. Do not add a placeholder draft provider or load the catalog, transcript bodies, or CAS state.

Edge cases include duplicate window ids, one thread claimed by two windows, one window claiming two threads, stale claim generations, a window referenced outside the active generation, empty restore sets, zero-runtime threadless initial records, missing selected-thread hooks, final-window removal, Exit racing ordinary close, and invalid placement metadata.

Verification must prove reverse uniqueness, generation-atomic publication, stale-claim rejection, exact empty-restore persistence, fallback retention after final ordinary close, bounded startup I/O independent of catalog size, and authoritative validation failure rather than silent substitution.

Implementation result: `beryl-state` now registers one exact-schema `beryl-session` domain containing the active header, fixed-size window records, claims by window, and claims by thread. Its revision-checked commands initialize the sole zero-runtime threadless window, create claimed windows, replace claims atomically, update placement, publish restoring generations, activate exact restoring claims, remove ordinary-close windows while retaining fallback, and mark dedicated orderly Exit. The V1 header is exactly 6,188 bytes, each V1 window record is exactly 655 bytes, the restore set is capped at 256 windows, every optional identity uses canonical tagged padding, and all-zero identities remain valid values.

Bootstrap result: `BerylStateBootstrap` registers and validates only the bounded session domain, exposes the header-to-exact-windows-to-header minimal query, rejects mixed publication, and completes registration only against the same Beryl home. Full `BerylState` registration composes that path. Unrelated malformed state is deliberately deferred until completion, while session corruption fails authoritatively before a window set is returned; no catalog, transcript, CAS state, draft placeholder, raw Fjall handle, or encoded record crosses the package boundary.

Verification result: all 53 `beryl-state`, 16 `beryl-model`, and 63 elevated all-feature `beryl-home-store` nextest cases pass. The session suite covers exact encodings and canonical padding, valid all-zero identity shapes, the 256-window hard bound, reverse claim disagreement and missing hooks, claims newer than the active generation, bounded stale-claim cleanup, exclusive ownership, exact revision/no-op rejection, atomic restore and activation, orderly-Exit gating, retained fallback after final close, unrelated-domain deferral, and cross-home bootstrap rejection. Locked scoped checks, warnings-denied Clippy and Rustdoc, formatting, metadata, dependency, source-size, forbidden-import, public raw-storage exposure, and whitespace audits pass. The full workspace all-target check still stops only at the declared Checkpoint 1 retained `beryl-app` theme-test cutover; no obsolete API was restored.

Resumable milestone: exact restorable window/session authority and its minimal pre-window read path are ready for later Syndic and GUI composition.

# Phase 9: Verify Integrated Storage, Concurrency, And Crash Recovery (finished)

- Register every Checkpoint 2 Beryl-state domain in one real home-store instance and exercise fresh open, durable mutation, close, reopen, validation, health failure, and same-home recovery as one target integration.
- Run the accepted concurrency matrix for competing revisions, different-domain commands, runtime/root uniqueness, session mutation, claim conflicts, settings Apply, job transitions, catalog rebuild state, and asset references.
- Run subprocess and deterministic fault tests for process abort, panic, and forced termination at every Beryl-controlled cut before commit, after commit before `SyncAll`, and after `SyncAll`, plus full-disk, denied/persistent I/O, disappearing storage, sidecar truncation/rename/sync failure, and repeated reopen failure. Literal termination inside Fjall's private batch-commit body remains part of the explicitly accepted issue #304 gap and must not be counted as covered.
- Verify package dependency and source boundaries, the absence of old-state readers and CAS-history fallback, and the still-explicit `beryl` bootstrap gap.

Edge cases include nondeterministic crash timing, cleanup code accidentally making an interrupted write look orderly, tests reusing one home concurrently, stale async results crossing home generations, rebuildable indexes masking authoritative corruption, and platform capability skips that would leave a required contract unproved.

Verification must pass formatting, docs and package policy, Cargo metadata/lock inspection, focused package tests, full new-foundation tests, forbidden-import scans, and explicit expected-failure proof for the unimplemented process entry. Checkpoint 2 proves fail-closed storage behavior and preservation of caller-owned coherent session snapshots; startup failure windows and live GPUI-window behavior remain Checkpoint 4 verification. A required platform case outside the accepted issue #304 exception that cannot be exercised or deterministically fault-injected blocks the phase rather than being silently skipped.

Implementation result: the permanent home-store fault controller now preserves exact injected I/O kinds and supports deterministic writer panics. An unwind from an admitted writer fails the health gate closed immediately; exact same-home recovery alone may cross the poisoned unit writer mutex and clears it only after publishing a fully validated replacement generation. New subprocess fixtures parent-kill blocked writers at every Beryl-controlled durability cut, while focused sidecar cases cover final-file truncation, the actual post-rename containing-directory barrier, and final verification. The literal inside-Fjall commit path remains the accepted issue #304 gap without an adapter or false coverage claim.

Integrated result: one public-API test populates every Checkpoint 2 Beryl domain in one home, crosses close/reopen, a surfaced indeterminate persistence failure, failed verification, same-home recovery, complete `BerylState::reacquire`, stale handle/command/sidecar-token rejection, and final reopen while retaining an unchanged caller-owned session snapshot. Deterministic concurrent commands cover different-domain and same-record revisions, executable and root uniqueness, session claims and close-versus-Exit, atomic Settings Apply, job lifecycle, catalog stale-versus-rebuild publication, and asset add-versus-remove indexes.

Verification result: all 72 elevated all-feature `beryl-home-store`, 61 all-feature `beryl-state`, and 16 `beryl-model` nextest cases pass. Locked scoped all-target checks, warnings-denied Clippy and Rustdoc, formatting, metadata and exact dependency inspection, source-size, forbidden-Fjall, raw-storage exposure, obsolete-reader, CAS-history-fallback, durable-byte-deletion, and whitespace audits pass. Exact Fjall 3.1.6 and its checksum remain unchanged. The targeted `beryl` check fails for the exact declared bootstrap compile gap, and the full workspace check reaches only that gap plus the already declared retained `beryl-app` theme-test cutover failures; no placeholder or obsolete API was introduced.

Resumable milestone: the target home store and Beryl-state foundations compile and pass focused ownership, durability, recovery, schema, and session verification without a compatibility path, with upstream issue #304 retained as the sole explicit accepted dependency gap.

# Phase 10: Independently Review Checkpoint 2 Completion (pending)

- Obtain an independent architectural completion review of the package graph, public API exposure, storage ownership, lock identity, revision and atomicity rules, durability barriers, health/reopen behavior, domain schemas, session bootstrap bounds, fault coverage, and rework boundaries.
- Address every finding through permanent target code, tests, documentation, or tracker/plan correction; do not introduce a shim, raw-engine escape hatch, weaker persistence mode, obsolete reader, or later-checkpoint placeholder.
- Mark Checkpoint 2 complete only when the independent review confirms the next checkpoint can build Syndic thread/draft and CAS projection state on these foundations without revisiting physical ownership.

Edge cases include an API that is typed but still leaks raw encoded bytes, a validator that drops unknown records, a success returned before `SyncAll`, a recovery path that changes home identity, an unbounded exact-catalog read, fixture-only behavior mistaken for production registration, and tests that never cross a process boundary.

Verification must rerun all Checkpoint 2 focused suites and boundary scans after review fixes, reconcile every checklist item in `REWORK.md`, and preserve the explicit later-checkpoint gaps.

Resumable milestone: Checkpoint 2 is independently verified complete; the durable plan may advance to Checkpoint 3 Syndic threads, durable drafts, and CAS projections.
