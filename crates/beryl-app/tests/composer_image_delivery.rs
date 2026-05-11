#![allow(dead_code)]

#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    io,
    path::{Path, PathBuf},
};

use beryl_app::{
    BerylWorkspacePersistence, WorkspaceImageAsset, WorkspaceImageAssetStatus,
    WorkspacePersistenceError,
};
use beryl_model::workspace::{BerylWorkspaceId, RuntimeMode};
use gpui::ImageFormat;

#[path = "../src/shell/composer_draft.rs"]
mod composer_draft;
#[path = "../src/shell/composer_image_delivery.rs"]
mod composer_image_delivery;

use composer_draft::{ComposerDraft, ComposerDraftImageData};
use composer_image_delivery::{
    RuntimeReadableImagePathError, prepare_accepted_composer_images, runtime_readable_image_path,
};

#[test]
fn host_runtime_uses_durable_asset_path_without_rewriting_image_bytes() {
    let (root, persistence, workspace_id, accepted) = durable_png_draft(b"png bytes");
    let prepared = prepare_accepted_composer_images(
        &persistence,
        &workspace_id,
        &accepted,
        &RuntimeMode::HostWindows,
    )
    .expect("host image path should resolve");

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].label(), "A");
    assert_eq!(
        std::fs::read(prepared[0].host_path()).expect("durable image should be readable"),
        b"png bytes"
    );
    assert_eq!(
        prepared[0].backend_path(),
        prepared[0].host_path().display().to_string()
    );

    cleanup_temp_dir(root);
}

#[test]
fn repeated_references_to_one_image_prepare_one_backend_path() {
    let (root, persistence, workspace_id, accepted) = durable_repeated_png_draft(b"png bytes");
    let prepared = prepare_accepted_composer_images(
        &persistence,
        &workspace_id,
        &accepted,
        &RuntimeMode::HostWindows,
    )
    .expect("host image path should resolve");

    assert_eq!(accepted.images().count(), 1);
    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].label(), "A");

    cleanup_temp_dir(root);
}

#[test]
fn image_without_durable_asset_id_is_rejected() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id();
    let accepted = accepted_png_draft(ComposerDraftImageData::new(
        ImageFormat::Png,
        b"png bytes".to_vec(),
    ));

    let error = prepare_accepted_composer_images(
        &persistence,
        &workspace_id,
        &accepted,
        &RuntimeMode::HostWindows,
    )
    .expect_err("non-durable image should be rejected");

    assert!(error.to_string().contains("does not have a durable"));
    cleanup_temp_dir(root);
}

#[test]
fn wsl_runtime_maps_drive_asset_path_to_selected_distro_mount_path() {
    let runtime = RuntimeMode::WslLinux {
        distro_name: "Ubuntu".to_string(),
    };
    let host_path = PathBuf::from(r"C:\Users\user\.beryl\workspaces\ws\image-assets\img.png");
    let mut probed = Vec::new();

    let path = runtime_readable_image_path(&runtime, &host_path, |path| {
        probed.push(path.to_path_buf());
        Ok(())
    })
    .expect("drive path should map into WSL");

    assert_eq!(
        path.backend_path(),
        "/mnt/c/Users/user/.beryl/workspaces/ws/image-assets/img.png"
    );
    assert_eq!(
        path.validation_path().to_string_lossy(),
        r"\\wsl.localhost\Ubuntu\mnt\c\Users\user\.beryl\workspaces\ws\image-assets\img.png"
    );
    assert_eq!(probed, vec![path.validation_path().clone()]);
}

#[test]
fn wsl_runtime_rejects_unmappable_host_asset_path() {
    let runtime = RuntimeMode::WslLinux {
        distro_name: "Ubuntu".to_string(),
    };
    let error =
        runtime_readable_image_path(&runtime, Path::new(r"\\server\share\img.png"), |_| Ok(()))
            .expect_err("UNC path should not map to /mnt");

    assert!(matches!(
        error,
        RuntimeReadableImagePathError::WslPathUnmappable { .. }
    ));
}

#[test]
fn wsl_runtime_rejects_unreadable_mapped_asset_path() {
    let runtime = RuntimeMode::WslLinux {
        distro_name: "Ubuntu".to_string(),
    };
    let host_path = PathBuf::from(r"C:\Users\user\.beryl\workspaces\ws\image-assets\img.png");
    let error = runtime_readable_image_path(&runtime, &host_path, |_| {
        Err(io::Error::new(io::ErrorKind::NotFound, "missing"))
    })
    .expect_err("unreadable WSL validation path should reject submission");

    match error {
        RuntimeReadableImagePathError::Unreadable {
            backend_path,
            validation_path,
            ..
        } => {
            assert_eq!(
                backend_path,
                "/mnt/c/Users/user/.beryl/workspaces/ws/image-assets/img.png"
            );
            assert_eq!(
                validation_path.to_string_lossy(),
                r"\\wsl.localhost\Ubuntu\mnt\c\Users\user\.beryl\workspaces\ws\image-assets\img.png"
            );
        }
        RuntimeReadableImagePathError::WslPathUnmappable { .. } => {
            panic!("drive path should be mappable")
        }
    }
}

fn durable_png_draft(
    bytes: &[u8],
) -> (
    tempdir_support::TestTempDir,
    BerylWorkspacePersistence,
    BerylWorkspaceId,
    composer_draft::AcceptedComposerDraft,
) {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id();
    let asset = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, bytes)
        .expect("asset should be created");
    let accepted = accepted_png_draft(ComposerDraftImageData::with_asset_id(
        ImageFormat::Png,
        bytes.to_vec(),
        asset.id(),
    ));
    (root, persistence, workspace_id, accepted)
}

fn durable_repeated_png_draft(
    bytes: &[u8],
) -> (
    tempdir_support::TestTempDir,
    BerylWorkspacePersistence,
    BerylWorkspaceId,
    composer_draft::AcceptedComposerDraft,
) {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = workspace_id();
    let asset = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, bytes)
        .expect("asset should be created");
    let mut draft = ComposerDraft::default();
    draft.sync_display_text(" then ".to_string());
    draft.replace_range_with_image(
        0..0,
        "A",
        ComposerDraftImageData::with_asset_id(ImageFormat::Png, bytes.to_vec(), asset.id()),
    );
    draft.replace_range_with_image(
        draft.display_text().len()..draft.display_text().len(),
        "A",
        ComposerDraftImageData::with_asset_id(ImageFormat::Png, bytes.to_vec(), asset.id()),
    );
    (
        root,
        persistence,
        workspace_id,
        draft.accepted().expect("image draft should be accepted"),
    )
}

fn accepted_png_draft(data: ComposerDraftImageData) -> composer_draft::AcceptedComposerDraft {
    let mut draft = ComposerDraft::default();
    draft.replace_range_with_image(0..0, "A", data);
    draft.accepted().expect("image draft should be accepted")
}

fn workspace_id() -> BerylWorkspaceId {
    BerylWorkspaceId::new("composer_image_delivery").expect("workspace id should be valid")
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-composer-image-delivery-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    let _ = root.close();
}
