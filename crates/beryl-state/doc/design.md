# Goals

Own Beryl's typed durable state schemas and bounded APIs for one Beryl home, while keeping
Syndic's intrinsic thread and history authority separate.

## Non-goals

- Opening, locking, or physically writing a home; owning Fjall; or exposing its storage machinery.
- Owning product workflows, GUI presentation, backend/CAS policy, Syndic threads or history, raw
  sidecar bytes, or physical theme-repository placement.
- Importing prior record forms, dual reads or writes, or adapters for noncurrent record formats.

# Decisions

## Documentation Set

This entry point governs these bounded normative supplements:

- [Runtime and session state](design-runtime-session.md) owns runtime/root, session/window, and
  reverse thread-claim records.
- [Theme and settings state](design-theme-settings.md) owns the typed theme repository service and
  typed scalar setting schemas.
- [Jobs and catalog state](design-jobs-catalog.md) owns durable-job valid state/transition records
  and compact catalog schema, normalization, indexes, and bounded query.
- [Asset state](design-assets.md) owns Asset V3 metadata, references, sidecar proof, witnesses, and
  repair participants.

## Public Boundary

- `beryl-state` registers its logical record families through `beryl-home-store`, owns their Rust
  owner and codec types, schemas, bounded typed reads, one-pass mutation preparation, revision
  rules, validation, and batch contributions. Stable family names and schema versions are durable
  declarations, not live type identity.
- It receives only opaque registered-domain handles: it never receives or exposes a raw database,
  keyspace, batch, transaction, byte encoding, writer guard, lock, or Fjall handle. Callers use
  stable Beryl and Syndic identities with expected revisions; physical keys and codec versions stay
  private.
- A Beryl-only command uses typed contributors. A command also affecting Syndic contributes both
  domains to one `beryl-home-store` command under the owning system's coordination. The asset
  owner-transfer participant exposes a checked maximum record and encoded-key-plus-value footprint
  derived from Asset V3 head shapes, including its marker-free validation-only form; it accepts no
  caller estimate and excludes other participants and physical-store overhead.
- An unpublished opening or reopening home candidate constructs the complete `BerylState` handle set. This
  package neither constructs the app/backend stack nor publishes it. Prior-generation handles,
  prepared commands, sidecar tokens, and receipts cannot authorize candidate or later work.
- Complete routine registration returns the session, runtime/root, settings, durable-job, catalog,
  asset and theme handles together or returns its typed failure. There is no session-only bootstrap
  facade or deferred completion operation. Routine registration remains distinct from explicit
  exhaustive schema validation; complete handles do not themselves publish the application stack.
- The package supplies the complete typed declaration of its required domains. Initial registration
  uses candidate-only authority, and candidate theme-service construction consumes exact home and
  generation metadata without requiring a prematurely healthy store.
- `BerylState::required_domains` returns that complete declaration. `BerylState::register` and
  `register_with_schema_validation` consume a mutable `HomeOpenCandidate` reference and preserve
  their routine versus exhaustive validation distinction.
- `AssetState::revision_candidate` reads only the asset-domain revision through borrowed explicit
  candidate recovery access. It preserves exact home/generation handle qualification and read
  confirmation failures; it neither grants ordinary admission nor publishes the candidate.
- `AssetState::owner_head_candidate` and `sealed_reference_set_manifest_candidate` expose the
  existing bounded typed owner and authenticated sealed-manifest reads through that same private
  access, preserving the [Asset proof contract](design-assets.md).

- `SessionState::revision_candidate` reads the session domain revision through explicit candidate
  recovery access, retaining typed home and generation checks without ordinary read admission.
- `RuntimeRootState::has_runtimes_candidate` checks runtime presence with a single bounded cursor
  page through explicit candidate recovery access. It preserves typed handle qualification and
  read failures without opening ordinary admission or retaining the registry.
- `SessionState::minimal_bootstrap_candidate` reads the bounded session header and exactly its
  referenced windows through borrowed explicit candidate recovery access, then rereads the header
  to reject mixed publication. It preserves the ordinary snapshot reader's identity/revision checks,
  typed home/generation qualification and read failures without granting ordinary admission.

## Outcomes, Reconciliation, And Validation

- Commands preserve `NotCommitted { evidence }`, `Committed { receipt, later_failure }`, and
  `Indeterminate { failure, reconciliation }` unchanged. `NotCommitted` proves no commit;
  `Committed` carries the exact home generation, home revision, and affected Beryl-domain revisions;
  `Indeterminate` alone carries the complete already-reserved registry capacity and opaque
  operation scope, no receipt, and authorizes no publication. Its sole custody moves synchronously
  unchanged to the home-store command coordinator.
- Each domain projects only its own receipt-bound revision through an opaque state handle, or `None`
  when unaffected. A bounded natural-record reconciliation hook classifies only its descriptor as
  `ExactOld`, `ExactNew` with reconstruction receipt facts, or `Collision`; mixed or unrelated
  observations are `Collision` and do not scan, merge, clear writer poison, or consult CAS.
- Routine open and handle reacquisition validate owner/codec types, durable declarations, required
  families, and generation without scanning application records. Explicit schema validation,
  whole-home scrub, or corruption investigation decodes every record envelope with exact stored-key
  legality and runs bounded domain validators. Direct home-store read failures, and asset sidecar
  failures, remain typed provenance; other rejection is semantic.

## Dependency Boundary

- This package may use `beryl-model`, `beryl-home-store`, serialization and validation support,
  Unicode normalization, and digest primitives needed by its schemas.
- It must not depend on `gpui`, `beryl-app`, `beryl-backend`, CAS protocol types, or
  `syndic-storage` private records. Cross-domain work uses stable public Syndic identities and typed
  home-store participants.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `persistent-state-integrity/v2`

The profile and modifier apply to this entry point and every linked supplement.
