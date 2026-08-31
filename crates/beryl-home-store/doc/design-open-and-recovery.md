# Physical Open And Recovery

This supplement is normative only for physical home open and layout, unpublished candidates,
structural lifecycle and health, same-home recovery, and whole-home scrub. It is governed by
[the package design](design.md), including that design's engineering-rigor contract.

## Physical Open And Layout

- Open input is one absolute Host path and one exact supported home-schema version. The home root
  contains the fixed ownership file `home.lock`; the sole Fjall database is the `state` directory.
- The package canonicalizes the configured path for process-local home identity and retains the
  configured spelling only for presentation and diagnostics. The configured root cannot itself be
  a Fjall database. The package does not retain filesystem-object identity or deny external
  replacement of the home or `state` directory.
- After acquiring the non-blocking exclusive OS lifetime lock and before opening Fjall, the package
  rejects an existing `state` symlink, junction, file, or other reparse-point collision without
  following it. It then creates or opens the ordinary directory and validates the exact home header
  and schema.
- A missing or empty `state` directory is fresh. A nonempty directory must contain Fjall's version
  marker and is force-recovered; it is never sent through a create-or-recover dispatcher.
- The reserved home header has one fixed-format encoding version, the exact home-schema version,
  and one randomly generated opaque `BerylHomeId`. Its generation and first persistence occur only
  after ownership lock acquisition.
- Native local NTFS is the fully supported crash-durability tier. UNC, WSL-backed, removable,
  synchronized, and other locations are best-effort only when basic access and a reliable exclusive
  lifetime lock succeed. The package does not conditionally prove remote durability.
- A published handle exposes only durable home id, schema, durability tier, configured and canonical
  paths, and diagnostic database path. It exposes no lock or database handle, keyspace, or encoded
  header bytes.

## Unpublished Open Candidates

- Initial open and every fresh same-home reopen first yield a private unpublished candidate, its
  filesystem tier, and candidate-only registration or reacquisition authority. The candidate is
  structurally `opening` or `reopening` and admits no ordinary command, read, sidecar, receipt
  projection, or application publication.
- Candidate-only bounded reads and commits exist solely for the owning system's recovery sequence.
  Partial domain registration and partial handles expose no session, restore set, or other
  application discovery. The one-shot publication capability is consumable only with the complete
  required Beryl and Syndic typed stack.
- Any disagreement, attachment-construction failure, or I/O failure publishes no candidate slot,
  registration, attachment, handle, or replacement state. It drops the provisional candidate and
  permits a later retry.

## Structural Health And Lifecycle

- Structural lifecycle is exactly `opening`, `healthy`, `failed`, or `reopening`. The operation
  gates in [Atomic commands](design-atomic-commands.md) are distinct and never become structural
  lifecycle states.
- State-dependent reads publish from their exact coherent snapshot only when selection,
  acquisition, envelope and decode work, generation, and affected operation gate all pass. They do
  not run a database-wide post-read health canary or reject coherent unaffected data because an
  unrelated maintenance transition raced them.
- Mutation, registration, recovery, sidecar publication, and other lifecycle-publication paths
  observe the Fjall operation and retained maintenance health relevant to their result. Direct
  bounded-policy denial remains nonstructural; corruption, integrity, keyspace-identity, poison,
  invalid registry, and other structural disagreement fail the generation. Exact dependency class,
  commit state, I/O kind, and original failure are retained before dependency types are erased.
- An unwind from admitted writer work fails the generation before writer admission drains.
  Recovery discards the poisoned writer and constructs a fresh writer.
- Clean close drops Fjall ownership and then releases `home.lock`. Ordinary drop does so only when
  no reconciliation custody remains. Reserved or installed custody may drop disposable Fjall state,
  but its self-retained registry core and lock custodian deny same-process reopen until terminal
  classification and explicit final close or process termination.
- Generation teardown closes attachment access and completes synchronous retirement as specified by
  [Domain API](design-domain-api.md). Prior-generation handles, commands, tokens, receipts, and
  asynchronous completions cannot authorize candidate work or later publication.

## Same-Home Recovery

- Recovery is single-flight and accepted only from `failed`. It retains the home lock and
  process-local reconciliation registry, drains and drops every Fjall and keyspace handle from the
  failed generation, and constructs a fresh service and fresh Fjall configuration for the same
  `state` directory. It never initializes replacement state over a failed home and requires no
  filesystem-object identity continuity.
- The fresh candidate has a new monotonic process-local home generation, private store-instance
  identity, cold dependency cache, and freshly constructed domain attachments. It validates the
  home header, registry, required declarations and physical families, and required Fjall physical
  structure before it can publish.
- Routine recovery is not a schema-validation boundary: it does not exhaustively scan application
  records or run whole-home scrub merely because a mutation was ambiguous. Retained operation
  descriptors reconcile only through freshly reacquired exact typed handles.
- Physical reopen success alone exposes no healthy application handle. The owning recovery system
  alone consumes the complete candidate and publishes the new typed stack; failure leaves the home
  `failed` and preserves the last coherent caller-owned state outside this package.
- The package exposes recovery-delay values `1`, `2`, `5`, `10`, and `30` seconds, remaining at
  `30` until success resets the sequence. Retry scheduling, GUI continuity, CAS repair, and service
  publication orchestration are not package responsibilities.

## Whole-Home Scrub

- Whole-home scrub is a separately requested or background bounded-memory operation, or follows a
  schema-validation boundary or corruption evidence. It is not routine open, same-home recovery,
  or acknowledgement-loss reconciliation.
- Exactly one scrub worker runs per home; concurrent requests join it, and evidence arriving during
  a run may coalesce into at most one pending rerun. Every terminal path releases its worker,
  snapshot, cursor pages, verifier state, and pending-rerun state before another run begins. This is
  a local cap, not a universal governor.
- Scrub streams every snapshot-current application envelope through the exact registered codec and
  bounded domain invariant state. It may invalidate and rebuild only projections whose domain
  contract declares them rebuildable; corrupt authoritative records fail validation instead of
  being silently dropped.
