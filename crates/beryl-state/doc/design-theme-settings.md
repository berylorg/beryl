# Theme and settings state

This supplement is governed by [the `beryl-state` design](design.md) and normatively owns the typed
theme repository service and Beryl-owned typed scalar settings only.

## Theme repository service

- The service owns stable installed theme ids, repository generations, manifest cursor pages,
  document observation revisions, byte lengths and digests, the finite role/property schema,
  bounded compact-TOML parser and validator, and complete resolved appearance values. Manifest
  membership has bounded names and order. An observed manifest identity binds repository generation
  plus exact physical byte length and digest, so an external same-generation rewrite cannot reuse a
  cursor or command identity. Every `installed/<stable-theme-id>.toml` observation binds exact
  length and digest; returning to an earlier digest still advances its process-local observation
  revision.
- Repository change hints are bounded and coalesced, name only manifest-admitted stable files, and
  are reread through the physical range API. Commands and rereads use the same parser, validator,
  and resolver. Invalid live edits retain the last coherent typed input; invalid startup content
  uses the built-in fallback input. Hints are never byte, revision, or commit proof.
- Install, rename, delete, reorder, update, Save, and Save As are revision-checked typed commands
  with exact `NotCommitted`, `Committed`, or `Indeterminate` outcomes and `ExactOld`, `ExactNew`,
  or `Collision` reconciliation. Before physical admission, a fixed-capacity shared clone registry
  reserves an operation slot. Indeterminate evidence and expected publication transfer there;
  callers receive only an operation id, one reconciliation flight is permitted, and `ExactOld` or
  `ExactNew` retire the scope while `Collision` keeps it closed for the service lifetime.
- Repository custody gates repository mutation and refresh; document custody gates overlapping
  mutation and reread of one id without blocking unrelated manifest pages. The composition-owned
  `BerylState` retains the service and gives shared clones. Its distinct bounded lifecycle registry
  has no global map or self-retaining owner and retires with its last clone and activity guard.
  Save As validates the original draft binding while publishing a copy under a new id; delete
  reacquires authoritative bounded settings and open-draft references before execution.
- Public reads are bounded point, range, or revision-bound pages; no API returns all installed
  themes or requires a whole document resident. A document load reobserves its manifest row and
  document identity after streaming and immediately before publication, yielding typed retry or
  freshness failure on a race. Diagnostics are content-free bounded counts only.

## Typed settings

- Settings records contain validated Beryl-owned scalar preferences, exact setting-schema versions,
  and record revisions. Theme documents and order remain in the repository service; only active
  theme identity and declared scalar theme settings use this domain. Backend Codex configuration,
  authentication, skills, MCP, sandbox, policy, and session state are rejected.
- V1 is the closed typed key set: active theme identity, context-compaction timeout,
  draft-autosave interval, developer instructions, and optional end-turn-sound Host path. There is
  no arbitrary key or byte-payload form. Active-theme identity is at most 256 UTF-8 bytes and
  developer instructions at most 60 KiB.
- Numeric scalars retain their caller-validated representation; feature authorities own ranges and
  defaults. In particular, context-compaction timeout is an exact caller-validated millisecond
  scalar whose accepted range, absent default, and operation meaning belong to the status-line
  feature; this package supplies no semantic default or expiry interpretation.
- A settings mutation is nonempty and duplicate-free, checks every absent-or-exact expectation
  before assembly, and advances all affected records atomically or none. Each key carries its exact
  supported schema version; an unknown schema or unsupported record version fails every decoded
  read and applicable exhaustive validation rather than becoming a default. Routine reopen does not
  scan dormant settings.
