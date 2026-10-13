use std::{num::NonZeroU64, sync::Arc};

use beryl_home_store::{
    HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, SidecarByteLimit,
    SidecarNamespace,
};
use beryl_model::{AssetId, HistoricalInputMarkerAssociationV1};
use beryl_state::{AssetHistoricalInputError, AssetOwner, BerylState};

use super::{
    fixture::{Fixture, IMAGE_BYTES, label, marker, publish_asset, with_association},
    source,
};

#[test]
fn foreign_home_verified_bytes_reject_even_for_the_same_asset_id() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    assert_eq!(foreign.asset, fixture.asset);
    assert_eq!(fixture.store.health().state(), HomeHealthState::Healthy);
    assert_eq!(foreign.store.health().state(), HomeHealthState::Healthy);
    let verified = foreign.verified();
    assert_ne!(verified.path(), fixture.sidecar_path);
    fixture.assert_unchanged(|| {
        let result = fixture
            .state
            .assets()
            .historical_input_marker_witness_with_verified_sidecar(
                &fixture.store,
                fixture.page(1),
                verified,
            );
        assert!(
            matches!(result, Err(AssetHistoricalInputError::Rejected)),
            "healthy foreign-home sidecar authority must reject, got {result:?}"
        );
    });
}

#[test]
fn another_namespace_or_asset_authority_cannot_replace_private_image_bytes() {
    let fixture = Fixture::new();
    let other_namespace = fixture
        .store
        .admit_sidecar(
            SidecarNamespace::new("other-images").unwrap(),
            IMAGE_BYTES,
            SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let namespace_authority = Arc::new(
        fixture
            .store
            .verify_sidecar(
                other_namespace.address(),
                SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
            )
            .unwrap(),
    );
    let (other, _) = publish_asset(&fixture.store, &fixture.state, b"different image authority");
    let other_authority = Arc::new(
        fixture
            .state
            .assets()
            .verify_sidecar(&fixture.store, other)
            .unwrap(),
    );
    for verified in [namespace_authority, other_authority] {
        fixture.assert_unchanged(|| {
            assert!(matches!(
                fixture
                    .state
                    .assets()
                    .historical_input_marker_witness_with_verified_sidecar(
                        &fixture.store,
                        fixture.page(1),
                        verified
                    ),
                Err(AssetHistoricalInputError::Rejected)
            ));
        });
    }
}

#[test]
fn same_digest_with_substituted_length_cannot_reuse_verified_authority() {
    let fixture = Fixture::new();
    let substituted = AssetId::sha256_v1(
        fixture.asset.digest(),
        NonZeroU64::new(fixture.asset.length().get() + 1).unwrap(),
    );
    let page = with_association(
        &fixture.page(1),
        HistoricalInputMarkerAssociationV1::new(1, marker(1), label(7), substituted).unwrap(),
    );
    fixture.assert_unchanged(|| {
        assert!(matches!(
            fixture
                .state
                .assets()
                .historical_input_marker_witness_with_verified_sidecar(
                    &fixture.store,
                    page,
                    fixture.verified()
                ),
            Err(AssetHistoricalInputError::Rejected)
        ));
    });
}

#[test]
fn retired_generation_authority_rejects_after_reopen() {
    let fixture = Fixture::new();
    let verified = fixture.verified();
    let page = fixture.page(1);
    assert_eq!(fixture.store.health().state(), HomeHealthState::Healthy);
    fixture.store.close().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        &fixture.path,
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    candidate.register_domain::<source::SourceDomain>().unwrap();
    let store = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .with_domain::<source::SourceDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let revision = store.home_revision().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    let head = state
        .assets()
        .owner_head(&store, AssetOwner::SubmittedTurnItem(fixture.item))
        .unwrap();
    let result = state
        .assets()
        .historical_input_marker_witness_with_verified_sidecar(&store, page, verified);
    assert!(
        matches!(result, Err(AssetHistoricalInputError::Rejected)),
        "closed-home sidecar authority must reject after healthy reopen, got {result:?}"
    );
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(
        state
            .assets()
            .owner_head(&store, AssetOwner::SubmittedTurnItem(fixture.item))
            .unwrap(),
        head
    );
}

#[test]
fn missing_sidecar_refuses_convenience_witness_with_no_record_changes() {
    let fixture = Fixture::new();
    let home_revision = fixture.store.home_revision().unwrap();
    let asset_revision = fixture.state.assets().revision(&fixture.store).unwrap();
    let owner = AssetOwner::SubmittedTurnItem(fixture.item);
    let head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, owner)
        .unwrap();
    let metadata = fixture
        .state
        .assets()
        .metadata(&fixture.store, fixture.asset)
        .unwrap();
    let held_path = fixture.sidecar_path.with_extension("held");
    std::fs::rename(&fixture.sidecar_path, &held_path).unwrap();
    assert!(matches!(
        fixture
            .state
            .assets()
            .historical_input_marker_witness(&fixture.store, fixture.page(1)),
        Err(AssetHistoricalInputError::Sidecar(
            beryl_home_store::SidecarError::Missing
        ))
    ));
    std::fs::rename(held_path, &fixture.sidecar_path).unwrap();
    let asset = fixture.asset;
    let store = fixture
        .store
        .recover_same_home()
        .unwrap()
        .publish()
        .unwrap();
    let state = BerylState::reacquire(&store).unwrap();
    assert_eq!(store.home_revision().unwrap(), home_revision);
    assert_eq!(state.assets().revision(&store).unwrap(), asset_revision);
    assert_eq!(state.assets().owner_head(&store, owner).unwrap(), head);
    assert_eq!(state.assets().metadata(&store, asset).unwrap(), metadata);
}
