# Goals

Own the typed durable schemas and APIs for Beryl-owned application state inside one Beryl home.

Provide revision-checked runtime/root, thread metadata, session/window, settings, catalog, durable-job, and asset-metadata domains over the shared physical home store without exposing storage-engine details or absorbing Syndic conversation ownership.

## Non-goals

- Opening or locking the Beryl home, owning Fjall, serializing the physical writer, or choosing persistence barriers.
- Owning Syndic threads, drafts, turns, items, projections, resources, or CAS-binding records.
- Owning product workflows, GPUI presentation, backend launch, CAS protocol, transcript rendering, or feature-specific GUI behavior.
- Importing workspace-era records, supporting compatibility schemas, or providing dual reads and writes.
- Owning installed theme documents or heavy asset and Syndic sidecar bytes.

# Decisions

## Public Boundary

- `beryl-state` registers the Beryl-owned logical keyspace families required by `doc/systems/beryl-home-storage/design.md` through the sealed typed-domain boundary exposed by `beryl-home-store`.
- It owns record schemas, codecs, domain validation, typed bounded queries, mutation validation, revision rules, and batch contributions for those keyspaces.
- It receives an opaque registered-domain handle. It never receives or exposes a raw database, keyspace, Fjall batch, transaction, writer guard, lock, or byte encoding.
- Callers use stable Beryl and Syndic identity values plus expected revisions. Stored key layout and codec versions remain private.
- Commands that affect only Beryl domains execute through this package's typed command contributors. Commands that must be atomic with Syndic changes contribute both domain mutations to one `beryl-home-store` command coordinated by the owning system.
- Successful mutation results contain the committed home revision and affected Beryl domain revisions. Conflicts, validation failures, health-gate failures, unsupported schemas, and persistence failures remain distinct typed outcomes.
- After successful same-home recovery, callers reacquire the complete `BerylState` handle set for the new generation. Handles, prepared commands, and sidecar tokens from the prior generation cannot authorize later work.

## Runtime And Root Domain

- Runtime records store stable runtime id, canonical absolute Codex CLI executable identity, derived exact Host or WSL-distribution mode, exact runtime-native executable path, user-facing environment label, creation facts, observed availability summary, and revision.
- Root records store stable root id, owning runtime id, canonical runtime-native path, user-facing full path, non-removable fact, availability summary, activity facts, and revision.
- Root admission rejects a directory whose canonical filesystem environment does not match the owning runtime's derived Host or WSL mode.
- One canonical Codex executable identity has at most one runtime record. Multiple executable runtimes may belong to the same Host or WSL environment.
- Canonically equivalent roots under one runtime cannot produce duplicate root records.
- Runtime creation plus its non-removable user-home root is one validated batch contribution; neither record may publish alone.
- Runtime and root records are additive; this package exposes no removal command contributor.
- Runtime and root ids, canonical paths, environment mode, availability observations, creation times, and activity times are already admitted caller facts. This package generates none of them and performs no filesystem, clock, WSL, process, or CAS observation.
- Every runtime and root record has a package-local nonzero monotonic record revision. Mutation contributors require its exact expected value as well as the enclosing home and domain revisions.
- Availability stores the bounded shared availability category plus an optional caller-supplied Unix-millisecond observation time. Unknown availability has no observation time; wall-clock order is never conflict authority.
- Root activity is only an optional caller-supplied Unix-millisecond presentation maximum. An update must be strictly later than the stored value.

## Thread Metadata Domain

- One thread-metadata record is keyed by stable Syndic thread id and stores exact immutable execution binding, generated-title metadata, automatic branch-discussion archive state, activity summary, token-usage presentation snapshot, and metadata revision.
- Beryl metadata never stores CAS names, CAS catalog rows, transcript bodies, draft text, turn content, or a duplicate Syndic tail.
- The package exposes generated-title updates and automatic branch-discussion archive transitions but no manual title, pin, archive, deletion, or execution-rebind contributor.
- Metadata record revisions are package-local nonzero monotonic values. The execution binding is accepted only at record creation and has no replacement mutation.
- Generated-title text is nonempty, free of control characters and surrounding whitespace, and at most 512 UTF-8 bytes. One accepted title stores its exact source Syndic thread revision and caller-supplied generation time and cannot be replaced or cleared.
- Activity summaries and token-usage snapshots store their exact source Syndic thread revision. Updates must strictly advance that source revision; token counters are nonnegative and a present model context window is positive.
- Archive state is exactly `ordinary`, `branch discussion open`, or `branch discussion archived`. Only open discussion metadata can transition to archived, and the archived value retains the exact successful handoff job id and caller-supplied archive time.
- The archive contributor is designed to join the exact handoff-job success contribution in one home command. This package does not independently infer handoff success.

## Session And Window Domain

- One `beryl-session` domain owns the active header, window records, claims by window, and claims by thread. Keeping those families together lets its ordinary reopen validator prove exact session and reverse-claim invariants through the sealed per-domain reader.
- The active session header stores generation, orderly-Exit intent, sorted unique restore-set window references, and the last successfully used runtime/root fallback retained when the restore set becomes empty. Each reference contains the exact window id and expected window-record revision.
- One home supports at most 256 restorable main-window records. The V1 header uses a fixed-capacity encoding and each V1 window record uses a fixed-size encoding with canonical tagged padding for optional identities and monitor facts; all-zero identities remain ordinary valid identities rather than absence sentinels.
- Each restorable main-window record stores window id, selected Syndic thread id, remembered runtime/root ids, placement and size facts, virtual-desktop identity when supported, and record revision.
- Auxiliary windows, flyouts, menus, notices, and previews never receive session records.
- Session commands preserve exclusive thread claims and reject missing or multiply referenced window/thread identities.
- The only valid threadless record is the sole zero-runtime initial window; it has no remembered target, selected thread, claim, or fallback.
- Minimal startup first registers and validates only the bounded four-family session domain. Its discovery query returns only the fixed-size session header and exact referenced window records, then rereads the header and rejects concurrent publication. This path registers no unrelated Beryl domain and returns no catalog, transcript, CAS, or placeholder-draft state.

## Thread Claims

- Claim records key exact `WindowId` and Syndic thread id with the publishing `SessionRevision`, active-or-restoring state, and a separate monotonic `ClaimRevision`.
- A window owns at most one active or restoring claim, and a thread has at most one active or restoring window owner.
- Claim, release, restore, and claim-or-create contributions validate both reverse indexes in the serialized home-store command.
- Stale asynchronous observations cannot clear or replace a newer claim generation.
- Paired claims outside the active header/window set are retained only as bounded stale startup state and are removed by the revision-checked begin-restore command. Missing or disagreeing reverse copies remain corruption.
- Claim records are durable coordination facts for crash/session recovery; they are not OS locks and do not replace the one-process home lock.

## Settings Domain

- Settings records store only validated Beryl-owned scalar preferences and stable setting-schema versions.
- Feature-owned setting schemas define accepted values and defaults; this package supplies typed storage and revision behavior without redefining those semantics.
- Applying multiple staged settings is atomic when the settings feature presents it as one Apply operation.
- Installed theme documents remain in the file-based theme repository. Only active theme identity and declared scalar theme settings use this domain.
- Backend-owned Codex configuration, authentication, skills, MCP, sandbox, policy, and session state are rejected from Beryl settings records.
- Schema V1 is a closed typed set: active theme identity, context-compaction timeout, draft-autosave interval, developer instructions, and optional end-turn-sound Host path. There is no arbitrary key or byte-payload variant.
- Active-theme identity is bounded to 256 UTF-8 bytes and developer instructions to 60 KiB. Numeric preference values are caller-validated feature scalars; this package preserves their exact typed representation without taking ownership of feature ranges or defaults.
- One Apply contribution is nonempty and duplicate-free, validates every absent-or-exact record expectation before mutation assembly, and advances every affected record atomically or none of them.
- Each key carries its exact supported setting-schema version. Unknown setting schemas and unsupported record versions fail decode or reopen rather than being interpreted as defaults.

## Durable Job Domain

- Durable job records own Beryl orchestration that must survive restart, including branch-resolution handoff admission, ordered attempts, exact target identities, idempotency keys, lifecycle state, bounded failure evidence, and job revision.
- Job schemas are typed by job kind. A generic payload blob cannot bypass owning-system validation.
- Job transition commands require the exact prior state and revision and cannot regress a terminal success or reuse an attempt identity.
- Job records store no authentication material, capability tokens, hidden developer instructions, or unbounded model/tool payloads.
- Schema V1 owns only branch-discussion handoff jobs. Each record retains exact intent, ordered attempt, child, parent, context owner and digest, resolving Syndic turn, correlated CAS tool request, parent queue ordinal, admitted resolution, parent Syndic/CAS identities when reached, lifecycle state, and job revision.
- Job ids are deterministically derived from the admitted resolution-intent identity. Exact CAS thread, turn, and tool-call identity forms the durable request-idempotency key; repeated delivery is answered from that index rather than creating another job.
- Lifecycle states are exactly `waiting_resolving_turn`, `waiting_parent`, `starting_parent`, `parent_active`, `retryable_failed`, `terminal_failed`, and `succeeded`. Retryable failure retains the exact checkpoint and resumes the same job and parent identities; terminal states leave the live-job index and never regress.
- Resolution text is bounded to 64 KiB and failure detail to 2 KiB. Failure kinds must agree with retryability and with the retained lifecycle checkpoint.
- Records, live jobs, request admissions, ordered discussion attempts, and latest-attempt pointers occupy separate typed families whose reopen validator proves their two-way agreement.

## Catalog Domain

- Catalog records are compact rebuildable projections keyed by Syndic thread id and deterministic recency indexes.
- Rows contain only generated-title precedence facts, runtime/root scope, automatic branch-discussion archive state, recent activity, claim/availability state, lineage summary, bounded title/environment/executable-path/root-path search normalization, source revisions, and catalog revision.
- Turn bodies, transcript items, draft content, Markdown, resource bytes, and CAS metadata are excluded.
- Rebuild inputs are bounded Syndic history summaries plus authoritative Beryl metadata. A catalog row never becomes the source of either domain's record.
- Complete cursor reads expose every matching compact row under explicit byte and row accounting while GUI row realization remains separately virtualized.
- Stale or invalid catalog projections are marked and rebuilt; they are never accepted as durable proof for correctness-sensitive mutation.
- Schema V1 stores one authoritative projection row keyed by Syndic thread id and one full compact row copy in a recent-first index. Recency keys invert the activity timestamp and use ascending thread id as the deterministic equal-time tie-break.
- Row facts preserve generated-over-Syndic-over-untitled title precedence, runtime and root ids and full paths, environment label, availability, claim, branch archive, bounded lineage, exact source revisions, projection freshness, and a package-local catalog revision.
- Search fields arrive already Unicode-normalized and case-folded from the owning projection system. This storage boundary validates text shape and byte ceilings without inventing a second normalization algorithm: 2 KiB for title, 1 KiB for environment label, and 64 KiB each for executable and root path.
- One row or recency record is bounded to a 256 KiB payload. Point and cursor results report exact stored key-and-value cost, and reads fail rather than silently returning an apparently complete collection outside caller bounds.
- Publishing a changed recency fact replaces the old index key in the same contribution. Marking stale updates the row and index copy together; reopen validation rejects missing, duplicate, or disagreeing copies.

## Asset Metadata Domain

- Asset records store opaque asset id, versioned digest and byte length, validated media facts, sidecar state, creation revision, and asset revision.
- Typed asset-reference records name exact owner kind and owner id without duplicating the owner's content.
- Sidecar bytes and runtime staging remain outside this package. First-reference and reference-removal commands coordinate metadata with the home-store sidecar ordering and owning durable-domain mutation.
- Removing the final known reference does not delete bytes.
- `AssetId` is the shared SHA-256-v1 digest plus exact nonzero byte length. Schema V1 admits at most 512 MiB per asset and retains an ASCII graphic media type of at most 127 bytes, optional nonzero dimensions, creation domain revision, committed-sidecar state, reference count, and metadata revision.
- Durable owner variants are current-draft marker, accepted input, submitted turn item, queued input, retry record, and transcript projection. Transient clipboard tokens are not written as durable references.
- First metadata publication is exposed only as a contribution bundle that also owns the matching admitted `images` sidecar token. Digest, length, namespace, and byte ceiling must agree before the home command can be assembled.
- Existing-asset reference addition and reference removal update the primary owner record, the per-asset index, metadata count, and revision in one contribution. Duplicate owners, mismatched asset identity, count overflow or underflow, and stale metadata revision reject the whole mutation.
- Removing the final reference retains zero-reference metadata and the sidecar. There is no metadata deletion or byte-deletion command.
- Reopen validation proves metadata, primary references, per-asset index copies, and counts agree, then verifies every retained sidecar's exact namespace, digest, length, and byte ceiling.

## Bounds And Validation

- Every non-fixed read requires explicit row, byte, or range limits. Exact catalog streaming additionally reports accumulated byte cost and stops with a typed bound failure if the documented compact-domain invariant is violated.
- Stored labels, paths, normalized search fields, failure evidence, setting values, and job payloads have schema-specific byte limits enforced before batch contribution.
- Domain validation checks runtime/root uniqueness, non-removable roots, metadata-to-thread identity shape, session/window references, claim reverse uniqueness, job state machines, catalog source revisions, and metadata-to-sidecar references.
- Reopen validation reports authoritative corruption instead of dropping records or consulting CAS history.

## Dependency Boundary

- This package may depend on `beryl-model`, `beryl-home-store`, serialization/validation support, Unicode normalization, and cryptographic digest primitives required by its owned schemas.
- It must not depend on `gpui`, `beryl-app`, `beryl-backend`, CAS protocol types, or `syndic-storage` private record types.
- Cross-domain coordination uses stable public Syndic identities and typed `beryl-home-store` participants rather than one package reaching into another package's records.
