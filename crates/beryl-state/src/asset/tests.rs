use std::num::NonZeroU64;

use beryl_home_store::{
    DomainCallbackSource, DomainRegistrationError, HomeCommand, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, SidecarByteLimit, SidecarError, SidecarNamespace,
};
use beryl_model::{AssetId, SyndicAcceptedInputId, SyndicDraftMarkerId};

use super::*;

mod batches;
mod codec;

fn open(path: &std::path::Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

fn sidecar_limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1_024 * 1_024).unwrap())
}

fn owner(byte: u8) -> AssetReferenceOwner {
    AssetReferenceOwner::AcceptedInputMarker {
        input_id: SyndicAcceptedInputId::from_bytes([byte; 16]),
        marker_id: SyndicDraftMarkerId::from_bytes([byte.wrapping_add(128); 16]),
    }
}

#[test]
fn first_reference_reopens_and_final_reference_removal_keeps_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = open(directory.path());
    let assets = AssetState::register(&mut store).unwrap();
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new(ASSET_NAMESPACE).unwrap(),
            b"durable image bytes",
            sidecar_limit(),
        )
        .unwrap();
    let path = sidecar.path().to_path_buf();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let domain_revision = assets.revision(&store).unwrap();
    let create = CreateAssetWithReference::new(
        asset_id,
        AssetMediaType::new("image/png").unwrap(),
        Some(AssetDimensions::new(
            NonZeroU64::new(10).unwrap(),
            NonZeroU64::new(20).unwrap(),
        )),
        domain_revision.checked_next().unwrap(),
        owner(1),
        UnixMillis::new(10),
    )
    .unwrap();
    let first = assets
        .create_with_reference(domain_revision, sidecar, create)
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    first.add_to(&mut command).unwrap();
    store.execute(command).unwrap();

    let metadata = assets.metadata(&store, asset_id).unwrap().unwrap();
    assert_eq!(metadata.reference_count(), 1);
    assert!(path.is_file());

    let mut remove = HomeCommand::new(store.home_revision().unwrap());
    remove
        .add(assets.remove_reference(
            assets.revision(&store).unwrap(),
            RemoveAssetReference::new(owner(1), asset_id, metadata.revision()),
        ))
        .unwrap();
    store.execute(remove).unwrap();
    assert_eq!(
        assets
            .metadata(&store, asset_id)
            .unwrap()
            .unwrap()
            .reference_count(),
        0
    );
    assert!(assets.reference(&store, owner(1)).unwrap().is_none());
    assert!(
        path.is_file(),
        "final-reference removal must not delete bytes"
    );

    store.close().unwrap();
    let mut reopened = open(directory.path());
    let reopened_assets = AssetState::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_assets
            .metadata(&reopened, asset_id)
            .unwrap()
            .unwrap()
            .reference_count(),
        0
    );
    assert!(path.is_file());
}

#[test]
fn first_reference_rejects_digest_or_length_disagreement_before_command_building() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = open(directory.path());
    let assets = AssetState::register(&mut store).unwrap();
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new(ASSET_NAMESPACE).unwrap(),
            b"one image",
            sidecar_limit(),
        )
        .unwrap();
    let wrong_id = AssetId::sha256_v1(
        [9; 32],
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let revision = assets.revision(&store).unwrap();
    let create = CreateAssetWithReference::new(
        wrong_id,
        AssetMediaType::new("image/png").unwrap(),
        None,
        revision.checked_next().unwrap(),
        owner(2),
        UnixMillis::new(1),
    )
    .unwrap();

    assert!(matches!(
        assets.create_with_reference(revision, sidecar, create),
        Err(AssetAdmissionError::IdentityMismatch)
    ));
}

#[test]
fn duplicate_owner_reference_is_rejected_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = open(directory.path());
    let assets = AssetState::register(&mut store).unwrap();
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new(ASSET_NAMESPACE).unwrap(),
            b"shared image",
            sidecar_limit(),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let revision = assets.revision(&store).unwrap();
    let create = CreateAssetWithReference::new(
        asset_id,
        AssetMediaType::new("image/png").unwrap(),
        None,
        revision.checked_next().unwrap(),
        owner(3),
        UnixMillis::new(1),
    )
    .unwrap();
    let first = assets
        .create_with_reference(revision, sidecar, create)
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    first.add_to(&mut command).unwrap();
    store.execute(command).unwrap();

    let metadata = assets.metadata(&store, asset_id).unwrap().unwrap();
    let mut duplicate = HomeCommand::new(store.home_revision().unwrap());
    duplicate
        .add(
            assets.add_reference(
                assets.revision(&store).unwrap(),
                AddAssetReference::new(asset_id, metadata.revision(), owner(3), UnixMillis::new(2))
                    .unwrap(),
            ),
        )
        .unwrap();
    assert!(store.execute(duplicate).is_err());
    assert_eq!(
        assets
            .metadata(&store, asset_id)
            .unwrap()
            .unwrap()
            .reference_count(),
        1
    );
}

#[test]
fn missing_referenced_sidecar_rejects_domain_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = open(directory.path());
    let assets = AssetState::register(&mut store).unwrap();
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new(ASSET_NAMESPACE).unwrap(),
            b"image that must remain",
            sidecar_limit(),
        )
        .unwrap();
    let path = sidecar.path().to_path_buf();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let revision = assets.revision(&store).unwrap();
    let create = CreateAssetWithReference::new(
        asset_id,
        AssetMediaType::new("image/png").unwrap(),
        None,
        revision.checked_next().unwrap(),
        owner(4),
        UnixMillis::new(1),
    )
    .unwrap();
    let first = assets
        .create_with_reference(revision, sidecar, create)
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    first.add_to(&mut command).unwrap();
    store.execute(command).unwrap();
    store.close().unwrap();

    std::fs::remove_file(path).unwrap();
    let mut reopened = open(directory.path());
    assert!(matches!(
        AssetState::register(&mut reopened),
        Err(DomainRegistrationError::ValidationAccess {
            source: DomainCallbackSource::Sidecar(SidecarError::Missing),
            ..
        })
    ));
}

#[test]
fn asset_byte_ceiling_rejects_metadata_before_storage() {
    let oversized = AssetId::sha256_v1([8; 32], NonZeroU64::new(MAX_ASSET_BYTES + 1).unwrap());
    assert!(matches!(
        CreateAssetWithReference::new(
            oversized,
            AssetMediaType::new("image/png").unwrap(),
            None,
            beryl_model::DomainRevision::new(1).unwrap(),
            owner(5),
            UnixMillis::new(1),
        ),
        Err(AssetMutationError::Value(
            AssetValueError::ByteBoundExceeded { .. }
        ))
    ));
}

#[test]
fn first_reference_requires_the_exact_resulting_domain_revision() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = open(directory.path());
    let assets = AssetState::register(&mut store).unwrap();
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new(ASSET_NAMESPACE).unwrap(),
            b"revisioned image",
            sidecar_limit(),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let revision = assets.revision(&store).unwrap();
    let create = CreateAssetWithReference::new(
        asset_id,
        AssetMediaType::new("image/png").unwrap(),
        None,
        revision,
        owner(6),
        UnixMillis::new(1),
    )
    .unwrap();

    assert!(matches!(
        assets.create_with_reference(revision, sidecar, create),
        Err(AssetAdmissionError::CreationRevisionMismatch { .. })
    ));
}
