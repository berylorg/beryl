#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

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
    transcript_image_path_resolver_for_assets, transcript_image_path_resolver_for_turns,
    trusted_host_path_for_backend_path,
};

#[test]
fn existing_durable_pasted_assets_resolve_host_source_and_wsl_runtime_paths() {
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
    assert_eq!(
        host_resolver
            .resolve_local_path(source_path)
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
        .unwrap();
    assert_eq!(
        wsl_resolver
            .resolve_local_path(runtime_path.backend_path())
            .and_then(|resolution| resolution.asset_id()),
        Some(asset.id())
    );
    cleanup(root);
}

#[test]
fn historical_direct_host_path_is_imported_as_a_durable_asset() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_host_import");
    let source_path = root.join("history-source.png");
    fs::write(&source_path, b"host png bytes").unwrap();
    let source_path = source_path.display().to_string();
    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &[turn_with_local_image(&source_path)],
    )
    .unwrap();

    let asset_id = resolver
        .resolve_local_path(&source_path)
        .and_then(|resolution| resolution.asset_id())
        .unwrap();
    assert_eq!(
        persistence
            .read_workspace_image_asset_bytes(&workspace_id, asset_id)
            .unwrap(),
        b"host png bytes"
    );
    cleanup(root);
}

#[test]
fn duplicate_direct_historical_paths_are_imported_once() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_duplicate_import");
    let source_path = root.join("history-duplicate.png");
    fs::write(&source_path, b"duplicate host png bytes").unwrap();
    let source_path = source_path.display().to_string();
    let turns = [
        turn_with_local_image(&source_path),
        turn_with_local_image(&source_path),
    ];

    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &turns,
    )
    .unwrap();

    assert!(resolver.resolve_local_path(&source_path).is_some());
    assert_eq!(
        persistence
            .load_workspace_image_assets(&workspace_id)
            .unwrap()
            .len(),
        1
    );
    cleanup(root);
}

#[test]
fn unsupported_historical_image_extension_is_not_imported() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_unsupported");
    let source_path = root.join("history-source.not-an-image");
    fs::write(&source_path, b"not an image").unwrap();
    let source_path = source_path.display().to_string();
    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &[turn_with_local_image(&source_path)],
    )
    .unwrap();

    assert!(resolver.resolve_local_path(&source_path).is_none());
    assert!(
        persistence
            .load_workspace_image_assets(&workspace_id)
            .unwrap()
            .is_empty()
    );
    cleanup(root);
}

#[test]
fn historical_path_without_a_directly_readable_source_stays_unresolved() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_unavailable");
    let source_path = "/tmp/cas-history/backend-source.png";
    let resolver = transcript_image_path_resolver_for_turns(
        &persistence,
        &workspace_id,
        &RuntimeMode::HostWindows,
        &[turn_with_local_image(source_path)],
    )
    .unwrap();

    assert!(resolver.resolve_local_path(source_path).is_none());
    assert!(
        persistence
            .load_workspace_image_assets(&workspace_id)
            .unwrap()
            .is_empty()
    );
    cleanup(root);
}

#[test]
fn wsl_posix_history_paths_translate_to_the_selected_distro_without_accepting_windows_paths() {
    let runtime = RuntimeMode::WslLinux {
        distro_name: "Ubuntu".to_string(),
    };
    assert_eq!(
        trusted_host_path_for_backend_path(&runtime, "/home/operator/image.png")
            .unwrap()
            .display()
            .to_string(),
        r"\\wsl.localhost\Ubuntu\home\operator\image.png"
    );
    assert!(trusted_host_path_for_backend_path(&runtime, r"C:\\foreign\\image.png").is_none());
}

#[test]
fn missing_existing_asset_keeps_an_unavailable_preview_state() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id("sources_missing_existing");
    let asset = persistence
        .import_workspace_image_asset(
            &workspace_id,
            ImageFormat::Png,
            b"png bytes",
            "/tmp/missing.png",
        )
        .unwrap();
    fs::remove_file(asset.file_path()).unwrap();
    let assets = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap();
    let resolver = transcript_image_path_resolver_for_assets(&RuntimeMode::HostWindows, &assets);

    assert_eq!(
        resolver
            .resolve_local_path("/tmp/missing.png")
            .unwrap()
            .preview_state(),
        TranscriptImagePreviewState::Unavailable
    );
    cleanup(root);
}

fn turn_with_local_image(path: &str) -> TurnInfo {
    serde_json::from_value(json!({
        "id": "turn_1",
        "items": [{
            "id": "user_1",
            "type": "userMessage",
            "content": [{ "type": "localImage", "path": path }]
        }],
        "status": "completed"
    }))
    .unwrap()
}

fn workspace_id(suffix: &str) -> BerylWorkspaceId {
    BerylWorkspaceId::new(suffix).unwrap()
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-transcript-image-sources-test-")
}

fn cleanup(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}
