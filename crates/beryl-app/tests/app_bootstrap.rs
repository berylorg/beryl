#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{fs, path::PathBuf, time::Duration};

use beryl_app::{
    AppBootstrap, AppBootstrapError, BerylHomeDir, DEFAULT_PROBE_TIMEOUT, StartupMetadata,
    StartupPersistenceError,
};
use beryl_model::workspace::WorkspaceId;

#[test]
fn bootstrap_defaults_to_startup_resolution() {
    let bootstrap = AppBootstrap::new(None);

    assert_eq!(bootstrap.initial_workspace(), None);
    assert_eq!(bootstrap.probe_timeout(), DEFAULT_PROBE_TIMEOUT);
    assert!(!bootstrap.memory_milestones_enabled());
    assert_eq!(
        bootstrap.beryl_home_dir().unwrap(),
        BerylHomeDir::from_environment().unwrap()
    );
    assert_eq!(bootstrap.window_title(), "Beryl");
}

#[test]
fn memory_milestones_are_opt_in() {
    let bootstrap = AppBootstrap::new(None).with_memory_milestones(true);

    assert!(bootstrap.memory_milestones_enabled());
}

#[test]
fn bootstrap_can_target_a_specific_workspace() {
    let workspace = WorkspaceId::host_windows(r"C:\work\beryl");
    let bootstrap = AppBootstrap::new(Some(workspace.clone()));

    assert_eq!(bootstrap.initial_workspace(), Some(&workspace));
    assert_eq!(
        bootstrap.window_title(),
        "Beryl - host-windows C:\\work\\beryl"
    );
}

#[test]
fn bootstrap_accepts_explicit_beryl_home_dir() {
    let root_dir = unique_temp_dir("explicit-bootstrap-root");
    let root = root_dir.join("state root");
    let bootstrap = AppBootstrap::new(None).with_beryl_home_dir(&root).unwrap();

    assert_eq!(bootstrap.beryl_home_dir().unwrap().root_dir(), root);
    assert!(!root.exists());
}

#[test]
fn bootstrap_resolves_relative_beryl_home_dir_against_current_directory() {
    let leaf = unique_leaf("relative-bootstrap-root");
    let relative = PathBuf::from(".")
        .join("target")
        .join("..")
        .join("target")
        .join(&leaf);
    let expected = BerylHomeDir::from_explicit_path(
        std::env::current_dir().unwrap().join("target").join(leaf),
    )
    .unwrap();

    let bootstrap = AppBootstrap::new(None)
        .with_beryl_home_dir(relative)
        .unwrap();

    assert_eq!(bootstrap.beryl_home_dir().unwrap(), expected);
    assert!(bootstrap.beryl_home_dir().unwrap().root_dir().is_absolute());
}

#[test]
fn beryl_home_dir_builds_stores_under_configured_root() {
    let root = unique_temp_dir("store-root");
    let home_dir = BerylHomeDir::from_explicit_path(&root).unwrap();

    assert_eq!(
        home_dir.workspace_persistence().workspaces_root(),
        root.join("workspaces")
    );
    assert_eq!(
        home_dir.gui_preferences_store().preferences_path(),
        root.join("preferences.toml")
    );
    assert_eq!(
        home_dir.appearance_settings_store().theme_path(),
        root.join("theme.toml")
    );

    home_dir
        .startup_persistence()
        .save(&StartupMetadata::default())
        .unwrap();
    assert!(root.join("startup-state.json").exists());

    cleanup_temp_dir(root);
}

#[test]
fn explicit_file_beryl_home_dir_fails_when_persistence_uses_root() {
    let root_dir = unique_temp_dir("file-root");
    let root = root_dir.join("not-a-directory");
    fs::write(&root, b"not a directory").unwrap();
    let home_dir = BerylHomeDir::from_explicit_path(&root).unwrap();

    let error = home_dir
        .startup_persistence()
        .save(&StartupMetadata::default())
        .unwrap_err();

    assert!(matches!(
        error,
        StartupPersistenceError::CreateDirectory { .. }
    ));
    fs::remove_file(root).unwrap();
}

#[test]
fn bootstrap_rejects_zero_probe_timeout() {
    let root = unique_temp_dir("zero-timeout-root");
    let error = AppBootstrap::new(Some(WorkspaceId::host_windows(r"C:\work\beryl")))
        .with_beryl_home_dir(&root)
        .unwrap()
        .with_probe_timeout(Duration::ZERO)
        .unwrap_err();

    assert_eq!(error, AppBootstrapError::ZeroProbeTimeout);
}

fn unique_temp_dir(label: &str) -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir(format!("beryl-app-bootstrap-{label}-"))
}

fn unique_leaf(label: &str) -> String {
    tempdir_support::temp_leaf(format!("beryl-app-bootstrap-{label}-"))
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    let _ = root.close();
}
