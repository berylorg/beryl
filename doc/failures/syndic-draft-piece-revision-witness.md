# Draft Build Revision Witnesses And Physical Faults

The bounded sequence repair initially treated all test-fault writes as mutations that advance the
Syndic domain revision. That claim was too broad: HomeStore's feature-gated
`inject_persisted_corrupt_record` in `src/fault/corruption.rs` directly inserts and persists one
codec-rejected physical envelope without updating either domain or home revision. Source-worker
inspection and independent root/reviewer inspection confirmed the write path.

A revision-sealed immutable cache therefore cannot use that raw physical seam as evidence that
every post-capture byte change invalidates its witness. Ordinary admitted Syndic mutations,
including the revisioned typed fault mutations, do advance the domain revision. The stale-witness
test uses a real admitted Syndic write; V3-envelope corruption tests run before a fresh acquisition
and prove canonical decode rejection instead.

The owning [sequence continuation contract](../../crates/syndic-storage/doc/design-draft-storage.md#bounded-sequence-range-continuation)
now states that distinction explicitly. It retains captured-domain-revision admission and the
existing immutable-publication trust boundary. It does not claim detection of raw physical
corruption after capture that bypasses revision publication. No HomeStore architecture change or
uncounted reread of sealed paths is justified by this test-seam distinction. Future retention or
fault mutations used to prove witness invalidation must publish the owning domain revision.
