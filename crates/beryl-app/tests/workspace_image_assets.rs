#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

use beryl_app::{BerylWorkspacePersistence, WorkspaceImageAssetStatus, WorkspacePersistenceError};
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest};
use gpui::ImageFormat;

#[test]
fn image_asset_creation_writes_original_bytes_and_reload_metadata() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("image_assets").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Images", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let asset = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, b"png bytes")
        .unwrap();

    assert_eq!(asset.status(), WorkspaceImageAssetStatus::Available);
    assert_eq!(asset.format(), ImageFormat::Png);
    assert_eq!(asset.metadata().byte_len(), 9);
    assert_eq!(fs::read(asset.file_path()).unwrap(), b"png bytes");

    let loaded = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id(), asset.id());
    assert_eq!(loaded[0].status(), WorkspaceImageAssetStatus::Available);
    assert_eq!(
        persistence
            .read_workspace_image_asset_bytes(&workspace_id, asset.id())
            .unwrap(),
        b"png bytes"
    );

    root.close().unwrap();
}

#[test]
fn imported_image_asset_roundtrips_source_backend_path() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("image_asset_import").unwrap();
    let source_path = "/tmp/cas-history/image-a.png";

    let asset = persistence
        .import_workspace_image_asset(
            &workspace_id,
            ImageFormat::Png,
            b"imported png bytes",
            source_path,
        )
        .unwrap();

    assert_eq!(asset.metadata().source_backend_path(), Some(source_path));
    let loaded = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();
    let loaded = loaded
        .iter()
        .find(|candidate| candidate.id() == asset.id())
        .unwrap();
    assert_eq!(loaded.metadata().source_backend_path(), Some(source_path));
    assert_eq!(
        persistence
            .read_workspace_image_asset_bytes(&workspace_id, asset.id())
            .unwrap(),
        b"imported png bytes"
    );

    root.close().unwrap();
}

#[test]
fn image_asset_reload_reports_missing_and_corrupt_files_explicitly() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("image_asset_status").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Images", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let missing = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, b"png bytes")
        .unwrap();
    fs::remove_file(missing.file_path()).unwrap();

    let corrupt = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Jpeg, b"jpeg bytes")
        .unwrap();
    fs::write(corrupt.file_path(), b"short").unwrap();

    let loaded = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();
    assert_eq!(
        loaded
            .iter()
            .find(|asset| asset.id() == missing.id())
            .unwrap()
            .status(),
        WorkspaceImageAssetStatus::MissingFile
    );
    assert_eq!(
        loaded
            .iter()
            .find(|asset| asset.id() == corrupt.id())
            .unwrap()
            .status(),
        WorkspaceImageAssetStatus::CorruptFile
    );

    assert!(matches!(
        persistence.read_workspace_image_asset_bytes(&workspace_id, missing.id()),
        Err(WorkspacePersistenceError::MissingWorkspaceImageAssetFile { .. })
    ));
    assert!(matches!(
        persistence.read_workspace_image_asset_bytes(&workspace_id, corrupt.id()),
        Err(WorkspacePersistenceError::CorruptWorkspaceImageAssetFile { .. })
    ));

    root.close().unwrap();
}

#[test]
fn image_asset_reference_state_roundtrips_cleanup_metadata() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("image_asset_refs").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Images", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let asset = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, b"png bytes")
        .unwrap();
    assert!(
        persistence
            .mark_workspace_image_asset_unreferenced(&workspace_id, asset.id())
            .unwrap()
    );
    let unreferenced = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();
    let unreferenced = unreferenced
        .iter()
        .find(|candidate| candidate.id() == asset.id())
        .unwrap();
    assert!(unreferenced.metadata().unreferenced_at_millis().is_some());
    assert!(unreferenced.metadata().retained_at_millis().is_none());

    assert!(
        persistence
            .mark_workspace_image_asset_referenced(&workspace_id, asset.id())
            .unwrap()
    );
    let referenced = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();
    let referenced = referenced
        .iter()
        .find(|candidate| candidate.id() == asset.id())
        .unwrap();
    assert!(referenced.metadata().unreferenced_at_millis().is_none());

    assert!(
        persistence
            .mark_workspace_image_asset_retained(&workspace_id, asset.id())
            .unwrap()
    );
    assert!(
        !persistence
            .mark_workspace_image_asset_unreferenced(&workspace_id, asset.id())
            .unwrap()
    );
    let retained = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();
    let retained = retained
        .iter()
        .find(|candidate| candidate.id() == asset.id())
        .unwrap();
    assert!(retained.metadata().retained_at_millis().is_some());
    assert!(retained.metadata().unreferenced_at_millis().is_none());

    root.close().unwrap();
}

#[test]
fn empty_image_asset_is_rejected_without_metadata() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("image_asset_empty").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Images", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    assert!(matches!(
        persistence.create_workspace_image_asset(&workspace_id, ImageFormat::Png, b""),
        Err(WorkspacePersistenceError::EmptyWorkspaceImageAsset)
    ));
    assert!(
        persistence
            .load_workspace_image_assets(&workspace_id)
            .unwrap()
            .is_empty()
    );

    root.close().unwrap();
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-workspace-image-assets-test-")
}
