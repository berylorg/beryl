use std::num::NonZeroU64;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeStore, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::AssetId;
use beryl_state::{AssetMediaType, BerylState, PublishAssetMetadata};

pub(super) fn publish_image_asset(home: &HomeStore, state: &BerylState) -> AssetId {
    let sidecar = home
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"\x89PNG\r\n\x1a\nphase54-repeated-steering-image",
            SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = state.assets();
    let revision = assets.revision(home).unwrap();
    let metadata = assets
        .publish_metadata(
            revision,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                None,
                revision.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    metadata.add_to(&mut command).unwrap();
    match home.execute(command) {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("active-steering image metadata command committed with later failure: {outcome:?}"),
        CommandOutcome::NotCommitted { evidence } => panic!("active-steering image metadata command was not committed: {evidence:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("active-steering image metadata command was indeterminate: {outcome:?}"),
    }
    asset
}
