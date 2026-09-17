# Reason For Investigation

Map replacement responsibilities onto Beryl's existing packages, persistent data and visible
behavior. Distinguish genuine simplification from duties transferred to the new runtime and
unfinished work already present before the CAS investigation.

# Outcome

Most product behavior and durable admission remain useful. The largest removals are native CAS
session/projection synchronization, lineage injection and provider repair mechanics. The largest
new persistent domain is exact model context and request/tool-effect custody. Existing content-
addressed sidecars do not already provide the proposed appendable final-resource path.

This is a disposition proposal, not authorization to delete packages or change schemas. The
[responsibility inventory](cas-responsibility-inventory.md) covers all 16 features and 10 systems;
the following maps those responsibilities to replacement work rather than repeating every contract.

## Package Dispositions

- **beryl-backend:** replace CAS release/executable admission, local listener protocol, native
  session/thread operations and historical repair transport. Retain or extract exact process
  supervision and operation-custody mechanics where they fit. Add subscription transport,
  account/catalog handling and direct operation outcomes under deliberately chosen boundaries.
- **beryl-stream and bounded-json:** assess incremental token/fragment parsing and bounded queues
  for reuse. Add SSE framing, dialect-aware item decoding and bounded late-identity handling.
  Existing JSON-RPC route decoding is not automatically the new protocol codec.
- **beryl-model:** retain provider-independent thread/input/resource and presentation identities;
  replace CAS route/generation/native-lineage metadata with explicit local request/context/tool
  identities. Avoid preserving obsolete types as an empty compatibility facade.
- **syndic-storage:** retain immutable history, drafts, exact admission, resource ranges, lineage,
  selected-path projections and reconciliation. Revise CAS source/repair schemas and add context
  selection, completed inference items and tool-effect outcomes without making rendered text the
  input source. Structured tool data currently has a closed typed algebra, not a generic JSON dump.
- **beryl-home-store:** retain lock, single writer, durability, typed domains, free-space admission,
  health generations and exact mutation reconciliation. Introduce any new domains explicitly;
  do not bypass the writer because a response arrived on an async worker.
- **beryl-state:** retain eligible UI/settings state and applied-versus-draft behavior. Runtime,
  account and model selections need revised types and validation if adopted.
- **beryl-app:** retain feature workflows, bounded transcript/rendering, app-tool brokers,
  notifications and window/job ownership. Replace CAS projection/session composition, provider
  activity mapping, title backend and runtime-specific Settings/recovery surfaces.
- **beryl executable:** compose the chosen runtime and retained services and preserve crash
  reporting/shutdown. Bootstrap currently has an intentional compile error; replacement does not
  inherit a proven complete executable graph.

Package names above locate work, not a final package split. Do not estimate savings by deleting
every file containing `cas`: some own required admission, stop, publication and cleanup semantics.

## Storage Constraint That Still Needs A Decision

For content-addressed sidecars, current home authority writes bytes to a unique temporary file, flushes,
renames to a digest-derived final path, synchronizes the directory and then commits metadata.
Asset identity is digest plus length. Existing-file reuse additionally compares the staged source
page by page. Temporary and orphaned files are retained pending future garbage-collection design.
This is confirmed in current storage authority, not an inference about the service.

Therefore request/item identity arriving early does not make the current sidecar API directly
appendable at its final digest path. A candidate request/item-owned appendable resource with a
later integrity seal would change identity/publication and incomplete-resource semantics. Another
option is retaining the existing sidecar construction for content addressing while keeping it
strictly separate from forbidden routing-order spooling. The Operator's acceptance of that storage
distinction must not be assumed if the selected target forbids all temporary payload files.

This constraint concerns digest-addressed sidecars. Canonical text already uses indexed bounded
records, and current history authority prohibits moving canonical Markdown into sidecars.
Suitability of that backing for new typed context resources requires separate assessment; not
every large model-context resource necessarily needs temporary-file construction.

No finding here establishes that subscription ordering inevitably requires spill. It establishes
that the end-to-end claim needs a storage decision: naming a temporary spool 'owned' does not
resolve it. A production prototype must show actual write destinations, sealing, visibility,
failure custody and eventual cleanup policy. Do not silently introduce garbage collection where
current authority deliberately retains unreachable data.

## Visible Product Changes

Composer acceptance/clear, queued input, replacement editing and incomplete-history presentation
remain. Steering timing needs an explicit direct-runtime contract. Transcript narrative stays
separate from raw reasoning/tool internals; generated images must enter asset admission instead
of trusting the old CAS `savedPath` producer route.

Status shows exact selected model/effort and reported usage, with unknown values preserved. Local
request closure is not turn completion. Local conversation-turn numbering can replace CAS view
numbers only after its product meaning is reconciled. Activity can use exact locally owned child
metadata, removing nickname/model discovery work without removing bounded retention.

Settings/runtime selection must stop implying that an executable path uniquely identifies the
inference backend. Keep host/WSL/root execution identity; decide account selection, login and
configuration exposure separately. Current Settings excludes backend credentials/configuration,
so their new controls require feature authority rather than being hidden in a transport change.

Startup without network or working authentication should preserve readable history and drafts;
inference eligibility can remain unavailable. Model catalog freshness and stale-account status
need truthful display. Do not perform inference simply to fill a status field or authenticate a
view. Exact recovery must not silently substitute another account, model, root or thread.

Titles remain independent maintenance with neutral instructions. Theme/lifecycle/branch tools
retain their brokers. Main-window close, Running threads, notifications and final shutdown remain
process-owned. Diagnostics should observe new runtime identities through ordinary paths rather
than retaining fictitious CAS status for display compatibility.

## Existing Data And Clean Replacement

Removing obsolete source architecture is not permission to delete Operator history. Existing
homes need a deliberate policy: a one-time schema/context conversion where exact source exists,
readable archived history with unavailable continuation where it does not, or an explicitly
chosen new-home boundary while retaining the old home. No option is adopted here.

Syndic narrative alone cannot reconstruct missing CAS-native reasoning/tool/compaction context.
A converter must distinguish exact recoverable input from approximated summaries. Do not promise
seamless continuation of every old thread or silently synthesize a replacement model context.
This research does not authorize scraping native sessions or importing private history.

No permanent dual-write/provider adapter is required merely to preserve data. Conversion can be
a bounded one-time operation under reconciled design, while the runtime remains a clean
replacement. Schema versioning, crash cuts and irreversible cutover belong to its later plan.

## Acceptance Limits

The disposition inventory is based on current authority and selected code anchors, not a
file-by-file deletion patch or line-count estimate. Storage implementation, old-home conversion,
GUI integration and complete startup remain unverified. Existing bootstrap, recovery integration,
branch coordination and deferred mounts must be costed separately from new runtime work.

# Sources

- Local authority/source inspected 2026-09-17: `Cargo.toml` and package manifests;
  `doc/systems/beryl-home-storage/design.md` Sidecar Commit Ordering;
  `doc/systems/image-assets/design.md` Asset Identity, Physical Byte Storage and Sidecar Admission;
  `doc/systems/syndic-conversation-history/design.md`; root and feature coverage in the
  [responsibility inventory](cas-responsibility-inventory.md).
- [Streaming assessment](ordering-and-bounded-streaming.md),
  [context assessment](context-and-compaction.md),
  [execution assessment](durable-execution-and-recovery.md) and
  [integration assessment](configuration-and-integrations.md) for new ownership and evidence limits.
