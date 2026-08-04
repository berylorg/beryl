use super::*;

#[test]
fn prepared_identity_mismatch_is_rejected_before_any_append() {
    let expected = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text("expected").unwrap()]).unwrap(),
    )
    .unwrap();
    let other = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text("other").unwrap()]).unwrap(),
    )
    .unwrap();
    let other_manifest = other.building_manifest();
    let forged = ContentManifestRecord::new(
        expected.id(),
        ContentRevision::new(1).unwrap(),
        other_manifest.encoding(),
        ContentLifecycle::Building,
        other_manifest.chunk_count(),
        other_manifest.encoded_bytes(),
        other_manifest.chain_digest(),
        other_manifest.expected(),
    );
    assert!(matches!(
        ContentAppend::prepare(&forged, &expected),
        Err(SyndicMutationError::ContentIdentityCollision)
    ));
}
