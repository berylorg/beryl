use std::{
    sync::mpsc::{self, Receiver},
    thread,
};

use beryl_model::workspace::BerylWorkspaceId;
use gpui::ImageFormat;

use crate::BerylWorkspacePersistence;

pub(super) enum TranscriptImagePreviewUpdate {
    Finished {
        request_id: u64,
        result: Result<TranscriptImagePreviewData, String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TranscriptImagePreviewData {
    format: ImageFormat,
    bytes: Vec<u8>,
}

impl TranscriptImagePreviewData {
    pub(super) fn format(&self) -> ImageFormat {
        self.format
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub(super) fn spawn_transcript_image_preview_worker(
    persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    asset_id: String,
    request_id: u64,
) -> Receiver<TranscriptImagePreviewUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = read_transcript_image_preview(&persistence, workspace_id, asset_id);
        let _ = sender.send(TranscriptImagePreviewUpdate::Finished { request_id, result });
    });
    receiver
}

fn read_transcript_image_preview(
    persistence: &BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    asset_id: String,
) -> Result<TranscriptImagePreviewData, String> {
    read_transcript_image_preview_from_persistence(persistence, &workspace_id, &asset_id)
}

pub(super) fn read_transcript_image_preview_from_persistence(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    asset_id: &str,
) -> Result<TranscriptImagePreviewData, String> {
    let asset = persistence
        .load_workspace_image_assets(workspace_id)
        .map_err(|error| format!("Beryl could not load image metadata: {error}"))?
        .into_iter()
        .find(|asset| asset.id() == asset_id)
        .ok_or_else(|| format!("Beryl could not find image asset {asset_id}."))?;
    let bytes = persistence
        .read_workspace_image_asset_bytes(workspace_id, asset_id)
        .map_err(|error| format!("Beryl could not read image bytes: {error}"))?;

    Ok(TranscriptImagePreviewData {
        format: asset.format(),
        bytes,
    })
}
