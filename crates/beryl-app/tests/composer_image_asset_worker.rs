#![allow(dead_code)]

#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    env,
    ffi::OsString,
    panic::{self, AssertUnwindSafe},
    path::Path,
    time::Duration,
};

pub use beryl_app::BerylWorkspacePersistence;
use beryl_model::workspace::BerylWorkspaceId;
use gpui::ImageFormat;

#[path = "../src/shell/composer_draft.rs"]
mod composer_draft;
#[path = "../src/shell/composer_image_assets.rs"]
mod composer_image_assets;

use composer_draft::ComposerDraftImageData;
use composer_image_assets::{ComposerImageAssetUpdate, spawn_composer_image_asset_worker};

#[test]
fn composer_image_asset_worker_writes_to_injected_root_when_environment_home_differs() {
    let env_home = unique_temp_dir("env-home");
    let injected_root = unique_temp_dir("injected-root");
    let workspace_id = BerylWorkspaceId::new("image_worker_injected_root").unwrap();
    let persistence = BerylWorkspacePersistence::new(&injected_root);

    let stored = with_environment_home(&env_home, || {
        let receiver = spawn_composer_image_asset_worker(
            persistence.clone(),
            workspace_id.clone(),
            ComposerDraftImageData::new(ImageFormat::Png, b"png bytes".to_vec()),
        );
        let ComposerImageAssetUpdate::Finished(result) =
            receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        result.unwrap()
    });

    let asset_id = stored.asset_id().unwrap();
    let asset = persistence
        .load_workspace_image_assets(&workspace_id)
        .unwrap()
        .into_iter()
        .find(|asset| asset.id() == asset_id)
        .unwrap();

    assert!(asset.file_path().starts_with(&injected_root));
    assert_eq!(
        persistence
            .read_workspace_image_asset_bytes(&workspace_id, asset_id)
            .unwrap(),
        b"png bytes"
    );
    assert!(!env_home.join(".beryl").exists());

    cleanup_temp_dir(injected_root);
    cleanup_temp_dir(env_home);
}

fn unique_temp_dir(label: &str) -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir(format!("beryl-composer-image-asset-worker-test-{label}-"))
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    let _ = root.close();
}

fn with_environment_home<T>(home: &Path, action: impl FnOnce() -> T) -> T {
    let userprofile = env::var_os("USERPROFILE");
    let home_var = env::var_os("HOME");
    unsafe {
        env::set_var("USERPROFILE", home);
        env::set_var("HOME", home);
    }

    let result = panic::catch_unwind(AssertUnwindSafe(action));

    restore_env_var("USERPROFILE", userprofile);
    restore_env_var("HOME", home_var);

    match result {
        Ok(value) => value,
        Err(payload) => panic::resume_unwind(payload),
    }
}

fn restore_env_var(key: &str, value: Option<OsString>) {
    unsafe {
        if let Some(value) = value {
            env::set_var(key, value);
        } else {
            env::remove_var(key);
        }
    }
}
