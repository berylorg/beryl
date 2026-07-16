# Goals

Provide shared pure-data identities and values used across Beryl packages without pulling in GUI, storage-engine, process, or protocol implementation.

## Non-goals

- Owning process launch, transport I/O, protocol parsing, or CAS requests.
- Owning GPUI rendering types or window lifecycle logic.
- Owning Fjall keyspaces, encodings, transactions, revision sequencing, or persistence workers.
- Owning Syndic turn, draft, item, projection, or CAS-binding record schemas.
- Owning product workflows or cross-package orchestration policy.

# Decisions

## Purity

- This crate must not depend on `gpui`, Tokio, Fjall, filesystem I/O, transport, or process-management APIs.
- Types are serializable only when a consuming boundary needs a stable value shape; serialization does not make this crate the owner of a stored schema.
- Pure validation may reject malformed identities, paths, bounds, and state combinations without performing availability probes or storage reads.

## Beryl Home And Window Identities

- `BerylHomeId` is an opaque stable identity derived and persisted by the home-store boundary; this crate does not derive it from user-facing path text.
- `WindowId` is an opaque stable identity for one restorable main conversation window.
- Pure session values may represent window geometry, monitor/work-area hints, virtual-desktop identity, selected Syndic thread id, and remembered runtime/root identity without owning restore policy.

## Runtime And Root Values

- `RuntimeId` is an opaque Beryl identity for one configured canonical Codex CLI executable.
- `RuntimeMode` preserves the Host or exact WSL-distribution environment derived from that executable path.
- Pure runtime values may carry canonical host-visible and runtime-native executable paths without probing them or deriving environment identity inside this crate.
- `RootId` is an opaque configured-root identity owned by one runtime.
- `ExecutionBinding` contains exact runtime identity and canonical runtime-native root path used by one Syndic thread.
- Availability is represented as explicit observed state with bounded reason categories. Pure types never probe a path, WSL distribution, executable, or CAS process.

## Thread Presentation Values

- Stable Syndic thread, turn, draft, draft-marker, content, item, accepted-input, retry-record, transcript-projection, resource, and execution-snapshot identities may cross package boundaries without making this crate the owner of their stored record schemas.
- `SyndicContentId` is the stable 128-bit lookup identity derived from an exact chunk-chain digest. `SyndicContentDigest` retains the complete 256-bit comparison authority so a truncated-identity collision is rejected rather than aliased.
- Idle submission preserves one exact 128-bit identity payload while changing its typed identity from draft to submitted turn. Queueing preserves the accepted-input identity and therefore has no separate queued-input identity type.
- Shared values may identify a Syndic thread, its execution binding, generated Beryl title metadata, automatic branch-discussion archive state, activity timestamp, parent-thread lineage summary, current window claim, and catalog availability.
- Beryl presentation metadata never contains CAS thread names or CAS catalog rows as authority.
- Thread title precedence is represented through explicit generated, Syndic-summary, and untitled sources rather than an inferred string.

## Revision And Command Values

- Opaque revision values identify home, domain, thread, draft, chunked-content manifest, accepted-input, per-thread input gate, projection, binding, claim, session, and job revisions without exposing storage-engine compare-and-swap primitives.
- `CasNativeTurnCount` is the zero-capable exact number of actual CAS model turns represented by a
  loaded thread prefix. Its checked increment and ordering are independent of Syndic
  conversation-DAG depth.
- Typed expected-revision sets and conflict reports may be shared across packages.
- Idempotency identities remain distinct from user-facing ids and CAS ids.
- Resolution-intent identity remains distinct from its derived durable handoff-job identity and from the external tool-call identity that admitted it.

## Provenance

- Shared provenance values distinguish user-authored input, Beryl-generated handoff input, CAS live events, dynamic tool calls, and durable recovery actions.
- Dynamic tool-call provenance may store exact app-server thread id, turn id, tool name, and tool-call id as opaque external identities.
- Provenance values must not contain authentication material, capability tokens, hidden developer instructions, or unbounded payload text.
- Exact CAS item ids, managed-process generations, loaded-thread generations, and distinct discussion-context, selected-path, and recovery-sequence digest domains may cross the backend, Syndic, and orchestration boundaries without owning provider calls or stored proof records.

## Asset Identity

- `AssetId` is a pure, versioned content identity composed of SHA-256 digest bytes and exact nonzero byte length.
- Product features treat asset identity as opaque; storage and sidecar boundaries may inspect its version, digest, and length to prove exact byte identity.
- The type owns no filesystem path, media metadata, reference record, sidecar operation, or garbage-collection policy.
