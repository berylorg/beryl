# Scope

Phase 13 recovered-history projection from Syndic through app orchestration into CAS
`thread/inject_items`.

# Invalidated Approach

Recovery first retained every selected-path turn, then every canonical item descriptor, then every
complete item text. App orchestration copied those items into a second backend-owned injection
batch before the already-buffered outbound JSON writer serialized them.

# Evidence

- `syndic-storage` used complete `Vec<TurnFrontier>` and `Vec<ItemFrontier>` collections, complete
  per-item byte/string allocations, and a final boxed item sequence.
- `beryl-app` mapped that sequence into another item vector.
- `beryl-backend` owned one boxed string per item plus the complete boxed batch.
- The existing 262,144-byte recovery product limit bounded the worst case but still made
  Beryl-owned residency and duplication proportional to accepted history size.

# Why It Failed

Fixed-buffer transport after complete source assembly is not an end-to-end bounded dataflow. The
shape also made exact revision and digest replay an after-the-fact batch property instead of a
source-to-wire proof.

# Course Correction

Syndic preflights the exact path with constant resident traversal and returns only compact totals,
revision, represented-prefix, and sequence-digest authority. A second revision-bound item/text
cursor replays fixed pages. App retains only that cursor plus one capacity-one broker page, and the
storage-neutral backend encoder rechecks ordinals, roles, lengths, totals, revision identity, and
the shared incremental digest while emitting the one logical JSON sequence.

The old item, whole-text, and injection-batch APIs are removed directly. Source disagreement after
fresh-target creation preserves completion ambiguity and forces target abandonment; it never
authorizes in-place replay.

# Authority And Verification

The correction is governed by `doc/systems/bounded-resource-dataflow/design.md`,
`doc/systems/cas-live-syndic-transcript/design.md`, Phase 13 sequence item 3 in `doc/plan.md`, and
the matching Beryl-home rework checklist item. Verification covers maximum counts, deep paths,
UTF-8 and JSON page boundaries, revision drift, digest disagreement, source loss, all injection
outcomes, and fixed resident high-water behavior.
