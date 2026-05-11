#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

pub use beryl_app::BerylWorkspacePersistence;
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest};
use gpui::ImageFormat;

#[path = "../src/shell/transcript_image_preview.rs"]
mod transcript_image_preview;

use transcript_image_preview::read_transcript_image_preview_from_persistence;

#[test]
fn transcript_image_preview_reads_durable_asset_bytes() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("preview_asset").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Preview", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    let asset = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, b"png bytes")
        .unwrap();

    let preview =
        read_transcript_image_preview_from_persistence(&persistence, &workspace_id, asset.id())
            .unwrap();

    assert_eq!(preview.format(), ImageFormat::Png);
    assert_eq!(preview.bytes(), b"png bytes");

    root.close().unwrap();
}

#[test]
fn transcript_image_preview_reports_missing_asset_bytes() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("preview_missing_asset").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Preview", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    let asset = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, b"png bytes")
        .unwrap();
    fs::remove_file(asset.file_path()).unwrap();

    let error =
        read_transcript_image_preview_from_persistence(&persistence, &workspace_id, asset.id())
            .unwrap_err();

    assert!(error.contains("Beryl could not read image bytes"));

    root.close().unwrap();
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-transcript-image-preview-test-")
}
