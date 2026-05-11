use std::{
    sync::mpsc::{self, Receiver},
    thread,
};

use beryl_model::workspace::BerylWorkspaceId;

use crate::BerylWorkspacePersistence;

use super::composer_draft::ComposerDraftImageData;

pub(super) enum ComposerImageAssetUpdate {
    Finished(Result<ComposerDraftImageData, String>),
}

pub(super) fn spawn_composer_image_asset_worker(
    persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    data: ComposerDraftImageData,
) -> Receiver<ComposerImageAssetUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = create_workspace_image_asset(&persistence, workspace_id, data);
        let _ = sender.send(ComposerImageAssetUpdate::Finished(result));
    });
    receiver
}

fn create_workspace_image_asset(
    persistence: &BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    data: ComposerDraftImageData,
) -> Result<ComposerDraftImageData, String> {
    let asset = persistence
        .create_workspace_image_asset(&workspace_id, data.format(), data.bytes())
        .map_err(|error| format!("Beryl could not store pasted image bytes: {error}"))?;

    Ok(ComposerDraftImageData::with_asset_id(
        data.format(),
        data.bytes().to_vec(),
        asset.id().to_string(),
    ))
}
