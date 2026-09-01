# Runtime and session state

This supplement is governed by [the `beryl-state` design](design.md) and normatively owns only
runtime/root, session/window, and thread-claim durable state.

## Runtime and root records

- A runtime records its stable id, canonical absolute Codex CLI executable identity, derived exact
  Host or WSL-distribution mode, runtime-native executable path, environment label, creation facts,
  availability summary, and nonzero monotonic record revision. A root records its stable id,
  runtime id, canonical runtime-native path, full path, non-removable fact, availability, activity,
  and revision.
- One canonical executable identity has at most one runtime. Canonically equivalent roots under one
  runtime have one root record. A root's canonical filesystem environment must equal its runtime's
  derived mode. Runtime creation and its non-removable user-home root are one validated atomic
  contribution; runtime and root records are additive only.
- Runtime/root ids, paths, modes, availability observations, and times are admitted caller facts:
  this package performs no filesystem, clock, WSL, process, or CAS observation. Availability is the
  bounded shared category plus optional Unix-millisecond observation time; unknown has none. Root
  activity changes only to a strictly later optional caller-supplied Unix-millisecond maximum.
- Every mutation requires the exact record, home, and domain revisions. A catalog join reads one
  exact runtime/root pair and validates its records and root-id index again in the serialized
  command before another domain publishes its projection.

## Session and windows

- The `beryl-session` domain owns one active header, restorable main-window records, and claims
  keyed both by window and by Syndic thread. One home has at most 256 restorable main windows. The
  V1 header is fixed-capacity; each V1 window record is fixed-size with canonical tagged padding
  for optional identities and monitor facts. An all-zero identity is valid, never an absence marker.
- The header stores generation, orderly-Exit intent, sorted unique restore-set `(window id,
  expected window revision)` references, and the last successful runtime/root fallback when the set
  is empty. A window stores its id, selected Syndic thread, remembered runtime/root, placement and
  size, supported virtual-desktop identity, and revision. Auxiliary windows, flyouts, menus,
  notices, and previews have no session records.
- The only valid threadless record is the sole zero-runtime initial window: it has no remembered
  target, selected thread, claim, or fallback. Minimal discovery reads only the fixed header and
  referenced windows, rereads the header, and rejects concurrent publication; it does not load
  catalog, transcript, CAS, or draft state.

## Reverse thread claims

- A claim records its exact window id and Syndic thread id in both reverse forms, publishing
  session revision, active-or-restoring state, and a separate monotonic claim revision. One window
  has at most one active/restoring claim and one thread has at most one active/restoring window.
  Claim, release, restore, and claim-or-create validate both reverse indexes in the serialized
  command; missing or disagreeing copies are corruption.
- Stale asynchronous observations cannot clear or replace a newer claim generation. Paired claims
  outside the active header/window set are only bounded stale startup state and are removed by the
  revision-checked begin-restore command. Claims are crash/session coordination facts, not OS locks
  and not a substitute for the one-process home lock.
- A catalog join uses one bounded claim point read proving matching reverse copies or thread-keyed
  absence, then carries that present-or-absent fact as an exact validation-only participant.

## Prepublication window abandonment

- The session boundary prepares one bounded exact-window abandonment candidate only for a claimed
  window that the caller identifies as acquired but not yet published visible. The candidate binds
  the current session revision, header generation, exact restore-set reference, window record, and
  both reverse claims to the caller's `WindowId` and private acquisition fingerprint.
- Its mutation contribution removes the exact restore-set reference, window record, and both
  reverse claims atomically and advances the session revision. A missing, changed, rebound,
  partially present, or disagreeing member rejects without deleting any session or claim record.
- Its natural-state read uses bounded point reads and classifies the session-owned closure only as
  exact acquired, exact abandoned, or collision. It does not inspect Syndic records, durable jobs,
  catalog rows, visibility, or application custody and cannot authorize a retry by itself.
