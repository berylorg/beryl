#[cfg(feature = "test-faults")]
use super::*;

#[cfg(feature = "test-faults")]
use beryl_home_store::DomainRegistrationError;
#[cfg(feature = "test-faults")]
use beryl_model::AssetReferenceSetDigest;
#[cfg(feature = "test-faults")]
use beryl_state::BerylStateRegistrationError;

#[cfg(feature = "test-faults")]
#[test]
fn reopen_rejects_owner_head_with_a_different_full_proof_for_the_same_set() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let proof = sealed_set(&store, state, publish_asset(&store, state));
    let owner = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([5; 16]));
    execute_asset(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                owner,
                None,
                Some(proof),
            )]))
            .unwrap(),
        ),
    );
    let head = state.assets().owner_head(&store, owner).unwrap().unwrap();
    let wrong = SealedAssetReferenceSetProof::new(
        proof.set_id(),
        proof.source(),
        proof.entry_frontier(),
        AssetReferenceSetDigest::from_bytes([u8::MAX; 32]),
    )
    .unwrap();
    execute_asset(
        &store,
        state.assets().corrupt_owner_head_proof_for_test(
            state.assets().revision(&store).unwrap(),
            owner,
            head.expectation(),
            wrong,
        ),
    );
    store.close().unwrap();

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let error = match BerylState::register(&mut reopened) {
        Ok(_) => panic!("corrupt owner head must fail asset-domain registration"),
        Err(error) => error,
    };
    let BerylStateRegistrationError::Domain { domain, source } = error else {
        panic!("expected asset-domain registration failure");
    };
    assert_eq!(domain, "beryl-assets");
    let DomainRegistrationError::Validation { domain, source } = source else {
        panic!("expected asset-domain invariant rejection, got {source}");
    };
    assert_eq!(domain, "beryl-assets");
    assert_eq!(
        source.to_string(),
        "asset owner head does not select its exact sealed set proof"
    );
}
