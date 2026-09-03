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
    assert_eq!(
        bootstrap.host_windows_standalone_app_server_executable(),
        None
    );
    assert!(!bootstrap.memory_milestones_enabled());
    assert_eq!(
        bootstrap.beryl_home_dir().unwrap(),
        BerylHomeDir::from_environment().unwrap()
    );
    assert_native_window_title(&bootstrap.window_title(), "Beryl");
}

#[test]
fn bootstrap_retains_raw_host_windows_standalone_app_server_executable() {
    let executable = PathBuf::from(r"relative folder\codex-app-server.exe");
    let bootstrap = AppBootstrap::new(None)
        .with_host_windows_standalone_app_server_executable(executable.clone());

    assert_eq!(
        bootstrap.host_windows_standalone_app_server_executable(),
        Some(executable.as_path())
    );
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
    assert_native_window_title(
        &bootstrap.window_title(),
        "Beryl - host-windows C:\\work\\beryl",
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

#[test]
fn workspace_open_forwards_standalone_host_app_server_without_wsl_fallback() {
    let shell_source = include_str!("../src/shell.rs");
    let discovery_source = include_str!("../src/shell/discovery.rs");
    let begin_workspace_open_body = rust_function_body(
        shell_source,
        "fn begin_open_target_with_thread_selection_and_intent(",
    );
    let launch_options_body = rust_function_body(
        discovery_source,
        "fn managed_backend_launch_options_for_execution_target(",
    );
    let open_workspace_worker_body =
        rust_function_body(discovery_source, "pub(super) fn open_workspace_worker(");

    assert!(begin_workspace_open_body.contains(
        "self\n            .bootstrap\n            .host_windows_standalone_app_server_executable()"
    ));
    assert!(begin_workspace_open_body.contains(
        "workspace_persistence_flush,\n                host_windows_standalone_app_server_executable,\n                timeout,"
    ));

    assert!(
        launch_options_body
            .contains("(beryl_model::workspace::RuntimeMode::HostWindows, Some(executable))")
    );
    assert_eq!(
        launch_options_body
            .matches("with_exact_host_windows_standalone_app_server")
            .count(),
        1,
        "only a Host-Windows target with a configured path may select the standalone executable"
    );
    assert!(launch_options_body.contains("_ => Ok(ManagedBackendLaunchOptions::default())"));

    let options_call = open_workspace_worker_body
        .find("let launch_options = managed_backend_launch_options_for_execution_target(")
        .expect("workspace open should derive options from its resolved execution target");
    let options_result = open_workspace_worker_body[options_call..]
        .find("host_windows_standalone_app_server_executable.as_deref(),\n        )?;")
        .expect("invalid Host-Windows standalone options must propagate with ?");
    let launch_call = open_workspace_worker_body[options_call..]
        .find("ManagedBackendServer::launch_and_probe_with_progress_and_options(")
        .expect("workspace open should launch with the selected options");
    assert!(
        options_result < launch_call,
        "standalone-option validation must complete before backend launch without a PATH fallback"
    );
    assert!(
        open_workspace_worker_body[options_call + launch_call..]
            .contains("launch_options,\n            timeout,")
    );
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

fn assert_native_window_title(title: &str, base_title: &str) {
    let build_id = title
        .strip_prefix(&format!("{base_title} \u{00b7} "))
        .expect("window title should retain its base label and build identity suffix");

    assert!(
        build_id == "unknown"
            || build_id
                .strip_suffix("-dirty")
                .unwrap_or(build_id)
                .bytes()
                .count()
                == 12
                && build_id
                    .strip_suffix("-dirty")
                    .unwrap_or(build_id)
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')),
        "build identity should be unknown or a lowercase twelve-hex commit with an optional dirty suffix: {build_id}"
    );
}

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    let signature_index = source
        .find(function_signature)
        .unwrap_or_else(|| panic!("missing function {function_signature}"));
    let after_signature = &source[signature_index..];
    let open_offset = after_signature
        .find('{')
        .unwrap_or_else(|| panic!("missing body for function {function_signature}"));
    let body_start = signature_index + open_offset;
    let mut depth = 0usize;

    for (offset, character) in source[body_start..].char_indices() {
        match character {
            '{' => depth = depth.saturating_add(1),
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &source[body_start..body_start + offset + character.len_utf8()];
                }
            }
            _ => {}
        }
    }

    panic!("unterminated body for function {function_signature}");
}
