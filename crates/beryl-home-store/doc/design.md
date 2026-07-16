# Goals

Own the reusable physical storage and process-ownership boundary for one Beryl home.

Provide typed, revision-checked, crash-durable coordination across registered Syndic and Beryl metadata domains without exposing Fjall internals to application or backend packages.

## Non-goals

- Owning conversation, draft, window, runtime, root, settings, catalog, job, or asset product semantics.
- Owning GPUI state, CAS launch, backend protocol, transcript rendering, or feature behavior.
- Importing workspace-era state or exposing compatibility reads and dual writes.
- Allowing callers to retain raw Fjall handles, keyspaces, batches, encodings, or writer guards.
- Preventing an external actor from rewriting Fjall child files in place or rolling back storage below the filesystem while preserving the same retained directory object; surfaced or validation-visible disagreement still fails closed.

# Decisions

## Public Boundary

- The package opens and locks one Beryl home according to `doc/systems/beryl-home-storage/design.md`.
- It owns the single Fjall `Database`, logical keyspace registration, serialized writer, persistence barriers, store-health state, and bounded typed read execution.
- Logical domains register private record families, exact codecs, validation hooks, typed reads, and typed mutation contributors through package-owned traits.
- Registration never gives a domain a raw database or keyspace handle.
- Each live domain blueprint, handle, command contribution, and reacquired recovery registration carries the exact process-local Rust owner type. Each family likewise carries the exact process-local codec type. Stable names and schemas remain durable compatibility facts, but cannot impersonate either live Rust owner; neither `TypeId` is persisted.
- Stable domain and family identifiers are bounded lowercase ASCII components. The persistent registry records the exact domain schema, complete sorted family declaration, exact family schemas, physical family names, and current domain revision; reopening rejects missing families or any incompatible declaration instead of creating or guessing it.
- Registering an already-persisted domain exhaustively validates every declared physical family and then runs its sidecar-aware domain validator before publishing a typed handle. Fresh registration persists an empty declared domain; explicit verification and recovery rerun exhaustive validation for every registered domain.
- Stored record values carry a store-owned exact record-version prefix. Domain codecs remain private to their package and application-facing APIs exchange only typed keys and values.
- Cross-domain commands name typed participants and expected revisions, then either commit one batch or return one typed rejection.

## Physical Open Contract

- Open input is one absolute Host path and one exact supported home-schema version.
- The home root contains the fixed retained ownership file `home.lock`; the sole Fjall database lives in the ordinary local directory `state`.
- The package opens the real home directory without share-delete permission, resolves its final path, and identifies the opened object by volume serial plus 128-bit file id. Configured path spelling is retained only for presentation and diagnostics.
- After acquiring `home.lock` and before opening Fjall, the package creates or opens the final `state` component without following a reparse point, validates its retained handle as an ordinary directory, flushes its home-directory link, and retains its volume serial, 128-bit file id, and no-delete handle outside every Fjall generation.
- Generic UNC targets, mapped remote drives, and targets whose opened-object identity or local-storage status cannot be established fail closed before Fjall is opened.
- A missing or empty `state` directory is fresh. A nonempty `state` directory must contain Fjall's version marker and is force-recovered; it is never passed through create-or-recover dispatch.
- The configured home root may not itself be a Fjall database, and `state` may not be a symlink, junction, file, or other reparse-point collision.
- The reserved home header contains one fixed-format encoding version, the exact home-schema version, and one randomly generated opaque `BerylHomeId`. Generation and first persistence occur only after the OS ownership lock succeeds.
- An opened handle exposes only the durable home id, schema, canonical live identity, configured and canonical paths, and the diagnostic database path. It never exposes the retained files, raw Windows handles, Fjall database, header keyspace, or encoded header bytes.
- Explicit close drops Fjall ownership, then the retained `state` directory, and only then unlocks `home.lock`; ordinary value drop provides the process-exit fallback.

## Inputs And Outputs

- Open input contains the configured home path and supported home-schema version.
- Open output is an opaque healthy home handle plus domain-specific typed handles.
- Busy, unsupported-schema, lock-unsupported, open, validation, conflict, persistence, sidecar, and health-gate failures remain distinct typed errors.
- Successful mutation output includes the exact process-local healthy home generation, committed home revision, and affected domain revisions needed by callers to reject stale asynchronous results.
- Receipt-bound domain revision access is admitted only against the exact current healthy store generation and matching typed domain handle. It returns `None` for an unaffected domain and a typed stale-or-foreign error for an obsolete generation rather than allowing revision values alone to authorize publication.
- Read APIs require explicit item, byte, or range bounds unless the result is a documented exact fixed-size record set such as the active session header.
- Cursor reads require two finite typed endpoints, materialize at most one caller-bounded page, report cumulative stored-byte cost and whether more matching records exist, and never return a Fjall iterator or guard.
- Read errors distinguish caller-produced key and result limits from malformed physical stored key and value envelopes. Caller limits leave health unchanged; a stored-envelope violation observed by an ordinary admitted read fails that generation structurally before another successful state-dependent result can publish.

## Atomicity And Durability

- The serialized writer validates expected revisions and each participant's exact live owner plus persistent registration immediately before commit.
- `CurrentDomainCommand` is an opaque single-domain boundary for mutations that already carry
  exact logical record fences. `execute_current` captures only that command's physical home and
  domain revisions after serialized writer admission, then uses the ordinary validation,
  contribution, batch, fault, persistence, receipt, health, cancellation, and reentry paths. It
  performs no retry and cannot combine domains or retain a sidecar token.
- `HomeCommand` remains the caller-fenced boundary for cross-domain and sidecar-retaining atomic
  work. A current-domain command is not a blind-write escape from record-level revision checks.
- Ordinary commands run only each participant's bounded mutation validation and contribution callbacks. They never rerun an exhaustive domain validator or scan unrelated records; one-record command work is independent of total domain size unless that mutation's own documented bounded reads reject.
- Registration, explicit verification, and recovery use a separate store-owned exhaustive path. It walks every physical key/value envelope with bounded memory, rejects empty or oversized keys and values before unbounded materialization, and delegates unknown, out-of-range, sentinel, version, and payload validation to the family's exact registered codec before the domain-level invariant callback runs.
- Callback errors explicitly separate typed `ReadError` or `SidecarError` access provenance from domain-owned semantic rejection. The store never guesses provenance by walking an erased error chain.
- A failure at any validation or contribution stage drops the complete uncommitted command.
- A successful correctness-sensitive mutation includes batch commit and `PersistMode::SyncAll` completion.
- The package never reports durable success before the required persistence barrier.
- Cooperative cancellation is accepted only before writer admission. Once admitted, a command runs to one durable success or typed failure result; same-thread writer reentry is rejected explicitly rather than deadlocking.
- Callers cannot hold transactions across await points or external work.
- Sidecar helpers enforce write, flush, atomic rename, directory durability where supported, then metadata-commit ordering.

## Health And Recovery

- The package exposes coherent `opening`, `healthy`, `verifying`, `failed`, and `reopening` states.
- Every state-dependent read, write, domain registration or reacquisition, and sidecar operation enters the same generation-aware admission gate. A surfaced storage or persistence failure whose durable outcome needs checking moves a healthy generation to `verifying`; malformed records, invalid trusted contracts, poisoned authority, and other structural disagreement move it directly to `failed`. Domain-owned semantic mutation rejection does not change health. Once admission closes, no newly admitted operation can publish state.
- An unwind from an admitted writer operation moves the store directly to `failed` before writer admission drains. Only exact same-home recovery may cross the poisoned unit writer mutex, and that poison is cleared only after a fully validated replacement generation is published; poison in registration or generation state remains fatal.
- Verification is single-flight, waits for already admitted work to drain, releases the ordinary writer before exhaustive work, performs `SyncAll`, validates the home header, control records, exact domain registry, every physical record envelope, domain invariants, and referenced sidecars, and either reopens the same generation as healthy or leaves the store failed.
- Recovery is single-flight and is accepted only from `failed`. It keeps the outer opened-directory, `home.lock`, and exact retained `state` object, drains every Fjall and keyspace handle from the failed generation, then reopens the final `state` component without following reparse points and requires the same complete opened-object identity before forced recovery. It never dispatches through create-or-recover, accepts a copied database merely because its header matches, or initializes replacement state.
- A recovered candidate must preserve the exact home identity and schema, reacquire every registered domain from its retained exact-owner blueprint, exhaustively validate all physical families plus domain and sidecar invariants away from the ordinary writer, and complete `SyncAll` before publication. Any disagreement or I/O failure leaves admission failed and permits a later retry.
- Successful forced recovery increments the monotonic process-local home generation and replaces the private store-instance identity. Handles, commands, sidecar-admission tokens, command receipts, and asynchronous completions from the obsolete generation cannot authorize work; callers reacquire typed domain handles through `HomeStore::domain_handle`, and receipt consumers validate the receipt against that exact current generation.
- The package exposes the accepted recovery delays as `1`, `2`, `5`, `10`, and `30` seconds, remaining at `30` seconds until successful recovery resets the schedule. Scheduling and preserving caller-owned in-memory GUI values remain application responsibilities.
- A health failure rejects new commands according to the system gate without closing application windows, mutating caller-owned coherent values, reading CAS, or inventing fallback data.
- Rebuildable domain projections may be invalidated and rebuilt only when their domain contract permits it.

## Sidecar Publication

- Sidecars live under `sidecars/<namespace>/<first-two-SHA-256-hex>/<full-SHA-256-hex>`. Typed durable metadata owns the namespace, digest, and exact byte length; every admission and verification also requires an explicit nonzero caller byte limit.
- Admission opens the canonical home, sidecar root, namespace, and shard as retained ordinary directories without following their final components. It flushes the exact parent link for the root, namespace, and shard on every attempt, including when each child already existed, so retry repairs an interrupted creation barrier rather than assuming it completed.
- Admission writes a unique temporary file, flushes its complete content, records its opened-object identity, closes it, and atomically renames it without replacement. Fresh publication, existing reuse, retry after failed publication, and an exact concurrent rename collision then converge on one path that retains an ordinary non-reparse final file, verifies exact length, SHA-256, and caller bytes, and flushes the retained shard before returning.
- A successful self-publisher additionally requires the retained final object to match the flushed temporary object's volume serial and 128-bit file id. Existing reuse and a verified no-replacement winner do not require another publisher's object identity, but no unrelated rename error is reclassified as a collision merely because the path exists.
- The returned `AdmittedSidecar` keeps that exact final file retained without write or delete sharing and is valid only for its healthy store generation. A metadata command that first references those bytes retains this token through its batch and `SyncAll` barrier; a failed or obsolete token cannot authorize metadata publication.
- Registered domains may use the bounded `SidecarVerifier` only during reopen validation to prove that their typed references still name ordinary retained final files with the declared length and digest.
- Failed admission may leave an inert temporary file or an unreferenced final file. The package never deletes either form and exposes no cleanup operation before the future home-wide garbage-collection design.

## Fault-Test Boundary

- The `test-faults` Cargo feature exposes deterministic actions only at concrete package call boundaries around reads, batch commit, persistence, verification, forced reopen, and sidecar file and directory operations. Production builds compile those checks to no-ops; there is no alternate storage engine, virtual filesystem, compatibility layer, or retry path.
- The feature additionally exposes one bounded persisted-corruption seam for post-registration read-health, verification, and recovery proofs. It requires the exact current typed domain handle and codec owner, accepts only a nonempty physical record envelope that the registered exact codec rejects, shares the existing explicit same-thread writer-reentry guard, serializes through the existing writer, and completes `SyncAll`.
- The corruption seam enforces fixed fixture-byte ceilings, exposes neither Fjall handles nor a reusable raw reader/writer, and rejects every envelope the exact codec would accept. It is absent from production builds and cannot bypass registration, recovery, or validation there.
- Package tests inject surfaced errors with exact I/O kinds, typed root/namespace/shard/final sidecar barriers, deterministic concurrency blocks, writer panics, subprocess aborts, parent-forced termination, callback-stage failures, closed-generation raw corruption, and bounded post-registration exact-codec-rejected envelopes. They prove exact owner and codec identity, bounded command work, writer-reentry rejection, exhaustive record-envelope rejection, ordinary-read fail-closed classification, admitted-read publication rejection, single-flight maintenance, old-or-new batch recovery, exact physical-state non-replacement, obsolete-generation rejection, retained-final object safety, and sidecar publication ordering at the boundaries Beryl controls.
- Fjall exposes no downstream failpoint inside its private journal write. The tests therefore do not claim to inject or observe the suppressed internal error described under Known Issues.

## Dependency Boundary

- This package may depend on Fjall and platform file-lock primitives.
- It must not depend on `gpui`, `beryl-app`, `beryl-backend`, or CAS protocol types.
- `syndic-storage` and Beryl metadata packages consume or register through this boundary without depending on one another's private records.

# Known Issues

## Fjall Batch Journal-Write Error Suppression

- The current approved dependency is the exact official Fjall 3.1.6 release.
- Fjall 3.1.6 `WriteBatch::commit` discards the fallible result of its journal `write_batch` call before applying and publishing the batch in memory. A transient or intermediate journal-write failure can therefore be followed by a successful persistence operation and Beryl `SyncAll` barrier even though recovery cannot reconstruct the complete batch.
- This dependency defect is tracked upstream as [fjall-rs/fjall#304](https://github.com/fjall-rs/fjall/issues/304). The Operator explicitly accepted using the official release with this known durability gap while awaiting the upstream response; whether to adopt a corrected release or maintain an owned fork remains deferred.
- Beryl still performs the required `SyncAll` barrier and fails closed for every error Fjall surfaces. It must not disguise this upstream gap with retries, dual writes, batch-size assumptions, or a compatibility adapter, and it must not claim that fault verification proves the suppressed-error path safe.
