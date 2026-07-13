# Goals

Own the reusable physical storage and process-ownership boundary for one Beryl home.

Provide typed, revision-checked, crash-durable coordination across registered Syndic and Beryl metadata domains without exposing Fjall internals to application or backend packages.

## Non-goals

- Owning conversation, draft, window, runtime, root, settings, catalog, job, or asset product semantics.
- Owning GPUI state, CAS launch, backend protocol, transcript rendering, or feature behavior.
- Importing workspace-era state or exposing compatibility reads and dual writes.
- Allowing callers to retain raw Fjall handles, keyspaces, batches, encodings, or writer guards.

# Decisions

## Public Boundary

- The package opens and locks one Beryl home according to `doc/systems/beryl-home-storage/design.md`.
- It owns the single Fjall `Database`, logical keyspace registration, serialized writer, persistence barriers, store-health state, and bounded typed read execution.
- Logical domains register private keyspace families, codecs, validation hooks, typed read commands, and typed mutation contributors through package-owned traits or sealed adapters.
- Registration never gives a domain a raw database or keyspace handle.
- Stable domain and family identifiers are bounded lowercase ASCII components. The persistent registry records the exact domain schema, complete sorted family declaration, exact family schemas, physical family names, and current domain revision; reopening rejects missing families or any incompatible declaration instead of creating or guessing it.
- Registering an already-persisted domain runs its sidecar-aware reopen validator before publishing a typed handle. Fresh registration persists an empty declared domain; recovery reruns the same reopen contract for every reacquired domain.
- Stored record values carry a store-owned exact record-version prefix. Domain codecs remain private to their package and application-facing APIs exchange only typed keys and values.
- Cross-domain commands name typed participants and expected revisions, then either commit one batch or return one typed rejection.

## Physical Open Contract

- Open input is one absolute Host path and one exact supported home-schema version.
- The home root contains the fixed retained ownership file `home.lock`; the sole Fjall database lives in the ordinary local directory `state`.
- The package opens the real home directory without share-delete permission, resolves its final path, and identifies the opened object by volume serial plus 128-bit file id. Configured path spelling is retained only for presentation and diagnostics.
- Generic UNC targets, mapped remote drives, and targets whose opened-object identity or local-storage status cannot be established fail closed before Fjall is opened.
- A missing or empty `state` directory is fresh. A nonempty `state` directory must contain Fjall's version marker and is force-recovered; it is never passed through create-or-recover dispatch.
- The configured home root may not itself be a Fjall database, and `state` may not be a symlink, junction, file, or other reparse-point collision.
- The reserved home header contains one fixed-format encoding version, the exact home-schema version, and one randomly generated opaque `BerylHomeId`. Generation and first persistence occur only after the OS ownership lock succeeds.
- An opened handle exposes only the durable home id, schema, canonical live identity, configured and canonical paths, and the diagnostic database path. It never exposes the retained files, raw Windows handles, Fjall database, header keyspace, or encoded header bytes.
- Explicit close drops Fjall ownership before unlocking `home.lock`; ordinary value drop provides the process-exit fallback.

## Inputs And Outputs

- Open input contains the configured home path and supported home-schema version.
- Open output is an opaque healthy home handle plus domain-specific typed handles.
- Busy, unsupported-schema, lock-unsupported, open, validation, conflict, persistence, sidecar, and health-gate failures remain distinct typed errors.
- Successful mutation output includes the committed home revision and domain revisions needed by callers to reject stale asynchronous results.
- Read APIs require explicit item, byte, or range bounds unless the result is a documented exact fixed-size record set such as the active session header.
- Cursor reads require two finite typed endpoints, materialize at most one caller-bounded page, report cumulative stored-byte cost and whether more matching records exist, and never return a Fjall iterator or guard.

## Atomicity And Durability

- The package validates expected revisions and registered invariants on the serialized writer immediately before commit.
- Every admitted command confirms the persistent registration still matches each typed handle, reruns every participating domain validator, then runs every contributor validation before assembling any pending mutation. A failure at any stage drops the complete uncommitted command.
- A successful correctness-sensitive mutation includes batch commit and `PersistMode::SyncAll` completion.
- The package never reports durable success before the required persistence barrier.
- Cooperative cancellation is accepted only before writer admission. Once admitted, a command runs to one durable success or typed failure result; same-thread writer reentry is rejected explicitly rather than deadlocking.
- Callers cannot hold transactions across await points or external work.
- Sidecar helpers enforce write, flush, atomic rename, directory durability where supported, then metadata-commit ordering.

## Health And Recovery

- The package exposes coherent `opening`, `healthy`, `verifying`, `failed`, and `reopening` states.
- Every state-dependent read, write, domain registration or reacquisition, and sidecar operation enters the same generation-aware admission gate. A surfaced persistence failure moves a healthy generation to `verifying`; structural disagreement moves it directly to `failed`. Once admission closes, no newly admitted operation can publish state.
- An unwind from an admitted writer operation moves the store directly to `failed` before writer admission drains. Only exact same-home recovery may cross the poisoned unit writer mutex, and that poison is cleared only after a fully validated replacement generation is published; poison in registration or generation state remains fatal.
- Verification is single-flight, waits for already admitted work to drain, performs `SyncAll`, validates the home header, control records, exact domain registry, registered domain invariants, and referenced sidecars, and either reopens the same generation as healthy or leaves the store failed.
- Recovery is single-flight and is accepted only from `failed`. It keeps the outer opened-directory and `home.lock` ownership handles, drains every Fjall and keyspace handle from the failed generation, and calls forced recovery only after proving the exact existing `state` layout remains present. It never dispatches through create-or-recover and never initializes replacement state.
- A recovered candidate must preserve the exact home identity and schema, reacquire every registered domain from the retained type-erased blueprint, rerun all ordinary and reopen validators, and complete `SyncAll` before publication. Any disagreement or I/O failure leaves admission failed and permits a later retry.
- Successful forced recovery increments the monotonic process-local home generation and replaces the private store-instance identity. Handles, commands, sidecar-admission tokens, and asynchronous completions from the obsolete generation cannot authorize work; callers reacquire typed domain handles through `HomeStore::domain_handle`.
- The package exposes the accepted recovery delays as `1`, `2`, `5`, `10`, and `30` seconds, remaining at `30` seconds until successful recovery resets the schedule. Scheduling and preserving caller-owned in-memory GUI values remain application responsibilities.
- A health failure rejects new commands according to the system gate without closing application windows, mutating caller-owned coherent values, reading CAS, or inventing fallback data.
- Rebuildable domain projections may be invalidated and rebuilt only when their domain contract permits it.

## Sidecar Publication

- Sidecars live under `sidecars/<namespace>/<first-two-SHA-256-hex>/<full-SHA-256-hex>`. Typed durable metadata owns the namespace, digest, and exact byte length; every admission and verification also requires an explicit nonzero caller byte limit.
- Admission writes a unique temporary file, flushes its complete content, closes it, atomically renames it without replacement, flushes the containing directory, then reopens and verifies exact length, SHA-256, and bytes. Existing content is reused only after the same verification.
- The returned `AdmittedSidecar` keeps the final file retained and is valid only for its healthy store generation. A metadata command that first references those bytes retains this token through its batch and `SyncAll` barrier; a failed or obsolete token cannot authorize metadata publication.
- Registered domains may use the bounded `SidecarVerifier` only during reopen validation to prove that their typed references still name ordinary retained final files with the declared length and digest.
- Failed admission may leave an inert temporary file or an unreferenced final file. The package never deletes either form and exposes no cleanup operation before the future home-wide garbage-collection design.

## Fault-Test Boundary

- The `test-faults` Cargo feature exposes deterministic actions only at concrete package call boundaries around batch commit, persistence, verification, forced reopen, and sidecar file and directory operations. Production builds compile those checks to no-ops; there is no alternate storage engine, virtual filesystem, compatibility layer, or retry path.
- Package tests inject surfaced errors with exact I/O kinds, deterministic concurrency blocks, writer panics, subprocess aborts, and parent-forced termination. They prove fail-closed admission, single-flight maintenance, old-or-new batch recovery, obsolete-generation rejection, non-replacement recovery, and sidecar publication ordering at the boundaries Beryl controls.
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
