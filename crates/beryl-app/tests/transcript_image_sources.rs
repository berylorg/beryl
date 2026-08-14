#![allow(dead_code)]

#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{collections::HashMap, fs, time::Duration};

pub use beryl_app::{
    BerylWorkspacePersistence, WorkspaceImageAsset, WorkspaceImageAssetStatus,
    WorkspacePersistenceError,
};
use beryl_backend::TurnInfo;
use beryl_model::workspace::{BerylWorkspaceId, RuntimeMode};
use gpui::ImageFormat;
use serde_json::json;

#[path = "../src/shell/composer_draft.rs"]
mod composer_draft;
#[path = "../src/shell/composer_image_delivery.rs"]
mod composer_image_delivery;
#[path = "../src/shell/execution_detail.rs"]
mod execution_detail;
#[path = "../src/shell/transcript_image_sources.rs"]
mod transcript_image_sources;

use execution_detail::TranscriptImagePreviewState;
use transcript_image_sources::{
    TranscriptImageExternalReader, transcript_image_path_resolver_for_assets,
    transcript_image_path_resolver_for_turns,
};

#[test]
fn existing_assets_resolve_host_source_and_wsl_runtime_paths() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_existing");
    let source_path = "/tmp/cas-history/source-a.png";
    let asset = persistence
        .import_workspace_image_asset(&workspace_id, ImageFormat::Png, b"png bytes", source_path)
        .unwrap();
    let assets = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();

    let host_resolver =
        transcript_image_path_resolver_for_assets(&RuntimeMode::HostWindows, &assets);
    let source_resolution = host_resolver
        .resolve_local_path(source_path)
        .expect("source path should resolve to imported asset");
    assert_eq!(source_resolution.asset_id(), Some(asset.id()));
    assert_eq!(
        host_resolver
            .resolve_local_path(&asset.file_path().display().to_string())
            .and_then(|resolution| resolution.asset_id()),
        Some(asset.id())
    );

    let runtime = RuntimeMode::WslLinux {
        distro_name: "Ubuntu".to_string(),
    };
    let wsl_resolver = transcript_image_path_resolver_for_assets(&runtime, &assets);
    let runtime_path =
        composer_image_delivery::runtime_readable_image_path(&runtime, asset.file_path(), |_| {
            Ok(())
        })
        .expect("durable asset path should map to WSL runtime path");
    assert_eq!(
        wsl_resolver
            .resolve_local_path(runtime_path.backend_path())
            .and_then(|resolution| resolution.asset_id()),
        Some(asset.id())
    );

    cleanup_temp_dir(root);
}

#[test]
fn missing_existing_asset_resolves_with_unavailable_preview_state() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_missing_existing");
    let source_path = "/tmp/cas-history/missing-existing.png";
    let asset = persistence
        .import_workspace_image_asset(&workspace_id, ImageFormat::Png, b"png bytes", source_path)
        .unwrap();
    fs::remove_file(asset.file_path()).unwrap();
    let assets = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();

    let resolver = transcript_image_path_resolver_for_assets(&RuntimeMode::HostWindows, &assets);
    let resolution = resolver
        .resolve_local_path(source_path)
        .expect("source path should still resolve through metadata");

    assert_eq!(resolution.asset_id(), Some(asset.id()));
    assert_eq!(
        resolution.preview_state(),
        TranscriptImagePreviewState::Unavailable
    );

    cleanup_temp_dir(root);
}

#[test]
fn historical_host_path_is_imported_without_backend_read() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_host_import");
    fs::create_dir_all(&root).unwrap();
    let source_path = root.join("history-source.png");
    fs::write(&source_path, b"host png bytes").unwrap();
    let source_path = source_path.display().to_string();
    let turns = vec![turn_with_local_image(&source_path)];
    let mut reader = FakeReader::default();

    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &turns,
        &mut reader,
        Duration::from_secs(1),
    )
    .unwrap();

    assert!(reader.calls.is_empty());
    let resolution = resolver
        .resolve_local_path(&source_path)
        .expect("host-readable source should be imported");
    let asset_id = resolution
        .asset_id()
        .expect("imported asset id should resolve");
    assert_eq!(
        persistence
            .read_workspace_image_asset_bytes(&workspace_id, asset_id)
            .unwrap(),
        b"host png bytes"
    );

    cleanup_temp_dir(root);
}

#[test]
fn historical_backend_path_is_imported_through_fs_readfile() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_backend_import");
    let source_path = "/tmp/cas-history/backend-source.png";
    let turns = vec![turn_with_local_image(source_path)];
    let mut reader = FakeReader::default();
    reader
        .files
        .insert(source_path.to_string(), b"backend png bytes".to_vec());

    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &turns,
        &mut reader,
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(reader.calls, vec![source_path.to_string()]);
    let resolution = resolver
        .resolve_local_path(source_path)
        .expect("backend-readable source should be imported");
    let asset_id = resolution
        .asset_id()
        .expect("imported asset id should resolve");
    let assets = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();
    let asset = assets.iter().find(|asset| asset.id() == asset_id).unwrap();
    assert_eq!(asset.metadata().source_backend_path(), Some(source_path));
    assert!(asset.metadata().retained_at_millis().is_some());
    assert_eq!(
        persistence
            .read_workspace_image_asset_bytes(&workspace_id, asset_id)
            .unwrap(),
        b"backend png bytes"
    );

    cleanup_temp_dir(root);
}

#[test]
fn duplicate_historical_paths_are_imported_once() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_duplicate_import");
    let source_path = "/tmp/cas-history/duplicate.png";
    let turns = vec![
        turn_with_local_image(source_path),
        turn_with_local_image(source_path),
    ];
    let mut reader = FakeReader::default();
    reader
        .files
        .insert(source_path.to_string(), b"backend png bytes".to_vec());

    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &turns,
        &mut reader,
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(reader.calls, vec![source_path.to_string()]);
    assert!(resolver.resolve_local_path(source_path).is_some());
    assert_eq!(
        persistence
            .load_workspace_image_assets(&workspace_id)
            .unwrap()
            .len(),
        1
    );

    cleanup_temp_dir(root);
}

#[test]
fn unsupported_historical_image_extension_is_not_imported() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_unsupported");
    let source_path = "/tmp/cas-history/image.not-an-image";
    let turns = vec![turn_with_local_image(source_path)];
    let mut reader = FakeReader::default();

    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &turns,
        &mut reader,
        Duration::from_secs(1),
    )
    .unwrap();

    assert!(reader.calls.is_empty());
    assert!(resolver.resolve_local_path(source_path).is_none());
    assert!(
        persistence
            .load_workspace_image_assets(&workspace_id)
            .unwrap()
            .is_empty()
    );

    cleanup_temp_dir(root);
}

#[test]
fn unrecoverable_historical_path_is_left_unresolved() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_unrecoverable");
    let source_path = "/tmp/cas-history/missing.png";
    let turns = vec![turn_with_local_image(source_path)];
    let mut reader = FakeReader::default();

    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &turns,
        &mut reader,
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(reader.calls, vec![source_path.to_string()]);
    assert!(resolver.resolve_local_path(source_path).is_none());
    assert!(
        persistence
            .load_workspace_image_assets(&workspace_id)
            .unwrap()
            .is_empty()
    );

    cleanup_temp_dir(root);
}

#[derive(Default)]
struct FakeReader {
    files: HashMap<String, Vec<u8>>,
    calls: Vec<String>,
}

impl TranscriptImageExternalReader for FakeReader {
    type Error = String;

    fn read_file_bytes(&mut self, path: &str, _timeout: Duration) -> Result<Vec<u8>, Self::Error> {
        self.calls.push(path.to_string());
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("missing {path}"))
    }
}

fn turn_with_local_image(path: &str) -> TurnInfo {
    serde_json::from_value(json!({
        "id": "turn_1",
        "items": [{
            "id": "user_1",
            "type": "userMessage",
            "content": [
                {
                    "type": "text",
                    "text": "Image A:"
                },
                {
                    "type": "localImage",
                    "path": path
                }
            ]
        }],
        "status": "completed"
    }))
    .unwrap()
}

fn workspace_id(suffix: &str) -> BerylWorkspaceId {
    BerylWorkspaceId::new(suffix).expect("workspace id should be valid")
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-transcript-image-sources-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    let _ = root.close();
}
