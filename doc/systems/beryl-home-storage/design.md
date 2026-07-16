# Goals

Define one durable Beryl-home storage system that keeps Syndic conversation state and Beryl application metadata physically efficient while preserving strict logical ownership, typed access, atomic mutation, and crash recovery.

Provide the process lock, session bootstrap, runtime/root registry, thread catalog projection, window claims, settings records, durable jobs, and failure gates needed by Beryl's features without exposing Fjall internals across package boundaries.

## Non-goals

- Allowing concurrent ownership of one Beryl home by multiple OS processes.
- Sharing one Syndic store between different Beryl homes.
- Importing workspace-era state or providing compatibility reads, dual writes, or migration adapters.
- Making the database, raw keyspaces, encodings, or Fjall transactions available to GUI or backend code.
- Storing CAS authentication, Codex configuration, app-server capability tokens, or backend-owned conversation history.
- Maintaining a second session snapshot outside Fjall.

# Decisions

## Participating Boundaries

- A new `beryl-home-store` package owns the physical Fjall database, process-wide writer, state-store health, home lock, and typed domain registration boundary.
- `beryl-state` owns the typed Beryl runtime/root, thread-metadata, session/window, claim, settings, durable-job, catalog, and asset-metadata schemas and command contributors registered through that physical boundary.
- `syndic-storage` owns typed Syndic records, codecs, queries, and mutations within its assigned keyspace family. It does not open a second database or expose raw Fjall handles.
- `beryl-app` consumes typed Beryl-home and Syndic APIs for shell orchestration. It does not own on-disk encodings or direct database access.
- `beryl-model` owns only pure identities and values shared across packages.
- `beryl-backend` does not access the Beryl-home store.
- Cross-domain atomic operations are expressed as typed home-store commands whose participants contribute validated mutations to one physical commit.

## Physical Home Layout

- One Beryl home owns exactly one Fjall database for durable Syndic records and Beryl application metadata.
- After the home lock is acquired and before Fjall opens, Beryl creates or opens the final `state` component without following a reparse point, validates an ordinary local directory, and retains its opened-object identity and no-delete handle for the complete store lifetime. A durable header with the same home id does not make another physical directory the same recovery candidate.
- The home also owns bounded sidecar directories for large image and Syndic resource payloads and the file-based installed-theme repository.
- Image bytes are ordinary content-addressed sidecar files under the Beryl home, never Fjall values or blob payloads. Fjall stores their typed metadata, durable references, and sidecar state.
- Scalar application settings, runtime/root configuration, thread presentation metadata, window/session state, durable orchestration jobs, catalog projections, and asset references live in Beryl-owned Fjall keyspaces.
- Installed theme documents remain in the theme repository; the active theme identity and other scalar theme settings live in Beryl settings records.
- Two configured home paths must not resolve to or contain the same physical database directory.
- Every stored record includes a schema version appropriate to its logical domain. Unsupported versions fail open or read with a typed error rather than being guessed or rewritten opportunistically.

## Keyspace Ownership

- Syndic keyspaces own threads, current drafts, submitted turns, canonical items, transcript views, projections, resources, source events, provider identities, CAS projection bindings, and Syndic revisions.
- Beryl runtime keyspaces own runtime records and configured-root records.
- Beryl thread-metadata keyspaces own execution bindings, generated titles, automatic branch-discussion archive state, token-usage presentation snapshots, and other GUI metadata keyed by stable Syndic thread id.
- One Beryl session domain owns the current restore-set generation, durable window records, selected thread ids, geometry, virtual-desktop identity, window-local remembered runtime/root values, exclusive main-window claims, and the last successfully used runtime/root fallback retained when the restore set becomes empty.
- Session, window, and both claim-index families share one typed domain so ordinary reopen validation can prove their forward and reverse invariants without a cross-domain raw-storage escape hatch.
- Beryl job keyspaces own durable resolution-handoff and other explicitly designed host-orchestration jobs.
- Beryl catalog keyspaces own compact rebuildable catalog rows and deterministic recency indexes derived from Syndic summaries plus Beryl metadata.
- Beryl settings keyspaces own validated app-wide scalar preferences. Feature-specific schemas remain owned by their feature and settings contracts.
- Beryl asset keyspaces own asset metadata and durable references. Asset bytes remain sidecars addressed through typed asset identities.
- No domain may scan, decode, mutate, or infer another domain's private records. Cross-domain joins and commits use explicit typed operations.

## Typed Domain Authority And Validation

- Every live logical-domain blueprint, handle, and command contribution is bound to its exact Rust owner type in addition to the durable stable name and schema. Every declared record family is bound to one exact codec type. These process-local identities are not persisted, and a same-name/schema type or alternate same-name codec cannot reacquire another live owner's authority.
- Registration of existing state, explicit health verification, and same-home recovery use one store-owned exhaustive validation path. It streams every physical record with bounded memory, checks physical key/value bounds, applies the exact owning codec to every key and versioned value envelope, and only then runs the domain's cross-record and sidecar invariants.
- Cursor-only sentinel keys may define finite typed query bounds but are illegal stored identities. Unknown, malformed, oversized, sentinel, and otherwise out-of-range raw keys fail exhaustive validation even when they lie outside every ordinary typed cursor range.
- Domain callbacks return storage-owned `ReadError` and `SidecarError` provenance through an explicit typed channel. Semantic domain rejection remains separate; the store never classifies health by searching an arbitrary erased error chain.
- Exhaustive work runs only during lifecycle maintenance, away from the serialized writer and GPUI thread. It may take total work proportional to durable domain size but retains only bounded records and state at once.

## Home Ownership Lock

- Before opening Fjall, Beryl creates or opens one fixed lock file inside the requested home and attempts a non-blocking exclusive OS file lock.
- The process retains the locked file handle until orderly shutdown completes. Normal release is explicit; handle release on process death is the crash fallback.
- Lock-file existence is not ownership evidence. A file left after a crash is reusable when its OS lock is no longer held, so no stale-lock deletion protocol is required.
- The lock decision is based on the opened file object, which prevents case, symlink, junction, and path-alias spellings from creating independent locks for the same physical file.
- Beryl canonicalizes the opened home directory for in-process identity after obtaining a real directory handle. User-facing paths may preserve their configured spelling, but ownership and registry keys use canonical identity.
- UNC homes are supported only when the filesystem supplies reliable exclusive file locking and durable rename/sync behavior. If those guarantees cannot be established, opening fails closed rather than weakening ownership.
- The Beryl home is a host-owned directory. WSL runtime paths never acquire or identify its lock.
- Diagnostic lock contents may include bounded process identity, but Beryl never trusts file contents as proof that the owner is alive and never breaks a live OS lock.
- The retained `state` directory handle is separate from a replaceable Fjall generation. It denies compatible rename, deletion, and replacement until every Fjall handle has drained and the home is being released, and recovery compares the live final path with that exact retained object before opening an engine candidate.

## Serialized Writer And Revision Checks

- One process-wide home-store writer serializes every Fjall mutation while bounded point and cursor reads may execute concurrently through typed read APIs.
- Serializing commits is an internal atomicity mechanism, not a durable database lock and not a restriction on simultaneous CAS turns for different Syndic threads.
- Every correctness-sensitive mutation carries the expected revisions of the records it read, including thread, draft, window claim, session, job, or binding revisions as applicable.
- A typed operation that mutates exactly one domain from exact logical record revisions may use a
  writer-admitted current-domain command. That opaque command captures only the physical home and
  domain revisions after it owns the serialized writer; it still validates every operation-owned
  logical revision, never retries semantic rejection, and cannot carry another domain or a
  sidecar. Unrelated commits therefore cannot make a live-event or projection command stale while
  it waits for writer admission.
- Caller-sampled physical revisions remain required for heterogeneous cross-domain and
  sidecar-retaining commands because their participants must share one explicitly prepared atomic
  basis.
- The writer validates all expected revisions, exact participant owner identities, and persistent registration declarations immediately before assembling the commit. It then runs only operation-bounded contributor validation and assembly; it never performs a whole-domain scan. A mismatch rejects the whole command with a typed conflict; it never merges opaque payloads or creates a competing same-thread child.
- One accepted command writes every participating keyspace mutation in one Fjall write batch, commits the batch, and completes the required persistence barrier before reporting durable success.
- Cancellation is authoritative only before writer admission. Once an operation enters the serialized writer, callers drain its outcome and never interpret a later cancellation request as proof that the batch did not commit.
- A surfaced storage or persistence failure after writer admission is an ambiguous durable outcome, not proof that the old records remain current. Callers gate publication, retain their coherent local user state, and wait for same-home verification or recovery before rereading the exact natural identities and revisions needed to reconcile whether the whole old or whole new atomic state became durable.
- Durable success returns an opaque receipt carrying the exact process-local healthy home generation, committed home revision, and revisions of only the participating domains. Domain owners project their own affected revision from that receipt; an unaffected domain yields no revision.
- Publication based on a successful asynchronous completion must validate its receipt against the exact current healthy generation and private store instance. Equal durable revision numbers do not make a prior-generation or foreign receipt current.
- A command that spans Syndic and Beryl metadata is all-or-nothing at the typed home-store boundary. Partial logical success is not published even if later background work such as CAS delivery has not yet occurred.
- Callers cannot hold a transaction or writer guard across asynchronous CAS, filesystem, windowing, or model work. External work uses prepare, short revision-checked commit, execute, and short result-commit steps with durable intent records where recovery is required.

## Durability Classes

- User input admission, draft flush, thread/draft creation, thread claim, runtime/root creation, generated-title or automatic-archive metadata mutation, session restore-set mutation, resolution intent, handoff-job transition, and CAS-binding transition require `SyncAll` before the caller may report them saved or accepted.
- A lifecycle flush waits for any already-admitted correctness-sensitive command and then reconciles or persists the latest exact state. It does not attempt to cancel admitted work or let close, Exit, switching, or submission continue from an ambiguous outcome.
- Live assistant deltas and other replayable stream updates may be coalesced into bounded short commits, but every published commit preserves a valid incomplete turn and uses `SyncAll`. A crash may lose only the uncommitted stream suffix.
- High-frequency window geometry and transcript-position state may be coalesced by durable key, but explicit Exit, ordinary window close, thread switch, and application shutdown create flush barriers.
- Rebuildable catalog indexes may lag their source records only when a durable stale marker makes that state explicit. Catalog publication waits for a coherent rebuilt snapshot rather than mixing source and stale index revisions.
- No caller may request weaker persistence for a correctness-sensitive class merely to reduce latency. A later design decision is required to weaken one of these guarantees.

## Sidecar Commit Ordering

- Large asset and Syndic resource bytes may live in content-addressed sidecar files under the Beryl home.
- Sidecar creation writes a new temporary file, flushes its contents, atomically renames it to its digest-derived final path, and flushes the containing directory where the platform supports that guarantee before committing a first durable metadata reference.
- Admission opens each sidecar ancestor without following its final component, validates and retains an ordinary directory through publication, and flushes the exact parent link for the sidecar root, namespace, and shard on every token-producing attempt. Existing directories receive the same bounded barriers so an earlier interrupted creation is repaired rather than assumed durable.
- Fresh publication, existing-file reuse, retry after an interrupted post-rename barrier, and concurrent no-replacement collision all converge on one retained final-object path. It rejects a reparse point or non-ordinary object, verifies exact content through that handle, and flushes the retained shard before an admission token exists.
- A successful publisher also requires the retained final file to have the same opened-object identity as its already-flushed temporary file. Another verified content-addressed publisher may win a no-replacement collision, but an identical-byte replacement between this publisher's rename and final open is not accepted as its publication.
- A committed metadata reference must never point at a sidecar that was not fully written and renamed.
- A crash may leave an unreferenced completed sidecar or temporary file. Such files remain inert until the future explicit garbage-collection design; ordinary startup does not guess that they are safe to delete.
- No sidecar bytes are deleted before the future explicit garbage-collection design, including after reference removal or when bytes appear unreachable.

## Store Health State

- The process-wide store health state is `opening`, `healthy`, `verifying`, `failed`, or `reopening`.
- A surfaced transient storage or persistence error rejects the current operation and moves a healthy store to `verifying` before another state-dependent command is admitted. Malformed durable records, invalid trusted codec or registration contracts, poisoned authoritative locks, missing required keyspaces, and sidecar invariant violations move it directly to `failed`. Semantic mutation rejection leaves a healthy store healthy.
- A panic unwinding through an admitted writer fails the store closed immediately. Same-home recovery may cross the resulting poisoned unit writer lock, but clears that poison only after it publishes a fully validated replacement generation; it never clears poisoned authoritative registration or generation state.
- Verification performs its persistence barrier plus bounded-memory exhaustive physical-family and domain-invariant validation away from both the serialized writer and GPUI thread. Any failed verification leaves the store `failed`; only complete success reopens the same generation.
- While `verifying`, mutating commands and reads that could publish new application state are gated. Already coherent in-memory windows remain intact.
- `failed` closes the shared Fjall admission gate for every operation identified by `doc/features/beryl-home/design.md` and retains each current window's identity, placement, selected thread, and last coherent resident surface in memory. The store does not fall back to CAS or cached projections as durable authority.
- On the transition to `failed`, CAS-live coordination uses the exact active-turn identities already held in memory to request best-effort interruption of every active CAS turn. It neither closes windows nor releases their thread claims when an interruption request fails or cannot be durably confirmed.
- CAS events, command results, and other incoming work observed while the Fjall gate is closed cannot be committed or published as durable Syndic state. The last successfully committed lifecycle record remains the recovery starting point.
- Recovery closes and reopens only the database in the exact retained physical `state` directory, reacquires each retained exact-owner domain blueprint and typed family, exhaustively validates required schema, every record envelope, domain invariants, and sidecars, and confirms a persistence barrier before returning to `healthy`.
- A failed recovery leaves the process in `failed`; it never initializes an empty replacement database at the same path.
- Running-session recovery uses one process-wide single-flight loop with delays of 1, 2, 5, 10, and 30 seconds, then repeats at 30 seconds while failure persists.
- A new failure signal while recovery is active joins that attempt instead of creating another loop. Successful validation cancels later retries and publishes one new healthy home generation.
- Publishing a new healthy generation invalidates every prior-generation typed handle, prepared command, sidecar token, and command receipt before any delayed completion may update caller-visible state.
- After successful same-home validation, CAS-live recovery reconciles every affected turn from its exact Syndic thread, Syndic turn, CAS thread, CAS turn, binding, and sequence identities before new turn work is admitted for that thread. It never guesses an identity, imports CAS history, or starts a second same-thread turn while exact terminal state remains unresolved.

## Reopen Validation

- Before domain-specific invariant reads, store-owned validation visits every physical key and value in every registered family through its exact codec. Typed cursor endpoints cannot hide malformed records outside an ordinary query range.
- Reopen validation checks the active session generation, window-record references, thread/draft uniqueness, committed-tail reachability, monotonic revisions, exclusive thread claims, CAS binding reverse uniqueness, pending-job state, catalog stale markers, and metadata-to-sidecar references.
- Rebuildable catalog projections may be marked stale and rebuilt from validated source records.
- Missing or corrupt authoritative thread, draft, session, job, binding, or asset-reference records fail validation rather than being silently dropped.
- Incomplete live work remains explicit through its owning lifecycle records and is recoverable without CAS historical reads.

## Runtime And Root Registry

- A runtime record has a stable Beryl id, one canonical absolute Codex CLI executable path, its derived Host or exact WSL-distribution mode, its exact runtime-native executable path, bounded availability facts, and a revision.
- Runtime and root ids are allocated by the admitting orchestration boundary before the short storage command begins. `beryl-state` validates and persists those already admitted identities; it does not probe filesystems, launch CAS, read a clock, or generate ids while holding writer admission.
- Canonical executable identity is unique within one Beryl home. Selecting an already registered executable resolves to the existing runtime record rather than creating a duplicate.
- A configured-root record belongs to one runtime and stores its canonical runtime-native directory path, user-facing path, availability facts, non-removable flag, and revision.
- Root admission requires the selected directory to resolve inside the owning runtime's derived Host or WSL environment; cross-environment paths are rejected rather than silently creating or selecting another runtime.
- Runtime creation and creation of its non-removable user-home root are one revision-checked durable command. Neither record becomes visible alone.
- A thread execution binding stores the runtime id and canonical root path needed for execution. It remains readable even if the configured-root record is later unavailable.
- Runtime and configured-root registries are additive. No runtime-removal, root-removal, or execution-rebind mutation is exposed.
- Runtime and root availability are observed facts, not destructive registry updates. Temporary absence does not erase configuration or thread bindings.
- Each runtime/root record owns a package-local monotonic record revision in addition to the enclosing domain revision. Availability updates require both expected revisions, retain a bounded availability category plus an optional caller-supplied Unix-millisecond observation time, and never use wall-clock time as mutation ordering authority.
- Root last-activity time is a presentation fact updated only by an exact later caller-supplied Unix-millisecond value. It does not become conversation or catalog authority.

## Thread Presentation Metadata

- Each thread-metadata record owns a package-local monotonic record revision and one immutable execution binding established when the record is created.
- An accepted generated title contains validated nonempty text of at most 512 UTF-8 bytes, the exact Syndic thread revision from which eligibility was derived, and a caller-supplied generation time. It is written at most once; later title generation cannot replace it.
- Activity summaries contain one caller-supplied Unix-millisecond activity time and their exact source Syndic thread revision. Token-usage snapshots contain exact nonnegative last and total counters, an optional positive model context window, an observation time, and their exact source Syndic thread revision.
- Activity and token-usage updates must advance the exact source Syndic thread revision and the metadata record revision. Stale, equal-revision, or out-of-order updates reject instead of overwriting newer facts.
- Automatic archive state is typed as ordinary, open branch discussion, or archived branch discussion. Only the one-way open-to-archived transition exists, and the archived fact records the exact successful handoff job id plus archive time.
- The branch-handoff coordinator contributes that archive transition only in the same home command that makes the exact handoff job successful. The metadata domain exposes no ordinary-to-archive, unarchive, manual title, pin, delete, or execution-rebind mutation.

## Thread Claims And Empty-Thread Acquisition

- A durable window id owns at most one active thread claim, and a Syndic thread has at most one active or restoring window claim.
- Each claim stores the exact session generation in which its current ownership/state was published and a separate monotonic claim revision. A later unrelated session publication does not rewrite an unchanged claim, but every claim-changing command validates both values as well as the current session revision.
- Claim-or-create is one serialized, revision-checked command over the exact runtime/root scope, catalog eligibility facts, thread record, current draft, and claim records.
- Eligible empty threads are ordered deterministically by oldest creation identity and then stable thread id. The first still-eligible unclaimed thread is reused; otherwise a new thread and empty current draft are created atomically.
- Claim eligibility is revalidated inside the writer. A stale catalog snapshot cannot cause two windows to acquire the same thread.
- Thread claim release is coupled to the accepted replacement claim or durable window removal so a visible window is not left threadless once a runtime exists.
- Claims belonging to the durable restore set remain restoring claims across process restart. Claims not referenced by the active restore generation are stale and are released during validated startup before ordinary acquisition.
- The initial zero-runtime window has a durable window record without a thread claim. Adding the first runtime atomically creates the runtime, home root, thread/draft, window claim, and selected-thread/session updates.

## Session And Window Records

- The store maintains one active restore-set generation and one bounded window record per restorable main conversation window, with at most 256 restorable main windows in one Beryl home.
- The active header is one fixed-capacity record whose sorted unique window references carry each exact window id and expected window-record revision. Window records use a fixed-size V1 encoding, including canonical padding for optional identities and the bounded monitor hint, so bootstrap byte cost is determined solely by the 256-window limit.
- A window record stores durable window id, selected Syndic thread id when allowed, remembered runtime/root, position, size, monitor/work-area hints, Windows virtual-desktop identity, and its record revision.
- The active session header retains the runtime/root pair from the latest successful thread activation or empty-thread acquisition independently of window-record lifetime. This fallback survives ordinary closure of the final window.
- Auxiliary Settings windows, flyouts, menus, notices, and previews never receive session records.
- Window creation, successful thread activation, accepted geometry updates, and ordinary window close update the active generation through revision-checked commands.
- Ordinary window close removes that window record only after required active-turn interruption and dirty-draft/session flush barriers succeed. Failure leaves the window open.
- Closing the final main window through ordinary close commits removal of the final window record, leaving the active restore set empty, and then terminates the process normally.
- Dedicated application Exit flushes the already-open window set and marks shutdown mode without processing those windows as ordinary closes.
- External process termination leaves the last `SyncAll`-completed active generation intact.
- In-memory window identity, placement, or selection retained during a failed store cannot survive process termination unless it was part of that last `SyncAll`-completed generation.
- Startup first runs the session domain's bounded reopen validator over its header, window, and two claim families; it does not register or validate unrelated Beryl domains. The following minimal discovery query reads only the active generation and referenced fixed-size window records, rereads the active header after those point reads, and rejects a concurrent publication rather than combining generations. After that set is known and before each restored window becomes visible, startup performs one bounded typed read for that window's selected thread and current draft.
- If validated session discovery finds an empty restore set and at least one runtime, startup chooses the most recently used runtime and its most recently used still-configured root, falling back to that runtime's non-removable home root. One command then creates the replacement window record and claims or creates an eligible empty thread in that scope before the window becomes visible.
- If the restore set and runtime registry are both empty, startup creates the one permitted threadless initial window record.
- The pre-window path does not load the full catalog, transcript records, or CAS state.
- Invalid or duplicate records fail session validation. Beryl does not silently substitute another thread or window identity.

## Compact Thread Catalog

- Each Syndic thread has one compact catalog row projection keyed by stable thread id and source revision.
- A row contains only bounded metadata required for generated-title precedence, runtime/root scope, automatic branch-discussion archive state, recent activity, availability, claim state, lineage summary, search normalization, and deterministic ordering.
- Turn bodies, transcript items, Markdown, resource bytes, draft text, and CAS thread metadata are excluded.
- Source mutations update the row in the same commit when the required facts are available, or atomically mark that row stale for bounded background rebuild.
- Catalog loading streams every row through bounded cursor reads into one complete in-memory metadata model and builds exact title, runtime, root, availability, and recency indexes without loading conversation bodies.
- Equal-recency order uses stable thread id as the final tie-breaker.
- Search uses precomputed Unicode-normalized case-folded title, runtime-label, configured-executable-path, and full-root-path fields. It is lexical filtering only and is not the deferred semantic-search feature.
- The complete compact model is exact domain state while resident. GUI row construction remains fixed-height and virtualized according to the conversation-thread GUI contract.
- An open flyout receives an immutable snapshot revision. Background catalog updates publish a later snapshot for the next interaction instead of reordering the open collection.

## Crash And Fault Verification

- Release verification includes subprocess harnesses that terminate writers at every Beryl-controlled cut before commit, after commit but before `SyncAll`, and after `SyncAll`, then reopen the same home and validate every domain invariant. Fjall exposes no deterministic downstream cut inside its private batch-commit body; the suppressed private journal-write error remains the explicit issue #304 gap and is never counted as covered.
- Fault injection covers full disk, denied writes, disappearing removable or network storage, truncated sidecar creation, failed rename, failed directory sync, process abort, panic, forced termination, repeated reopen failure, and bounded exact-codec-rejected physical record envelopes through the non-production `test-faults` boundary.
- Lock tests cover concurrent processes, process death with the lock file left in place, path case aliases, symlinks, junctions, UNC paths, and explicit orderly release.
- Concurrency tests cover competing draft saves, simultaneous different-thread submissions, claim-or-create races, session close versus Exit, resolution admission versus queued input, CAS binding transitions, and stale expected revisions.
- Tests assert that no failed command is reported accepted, no two windows own one thread, no thread has two current drafts or active turns, no durable metadata points at missing committed sidecar bytes, and no recovery path reads CAS history.
