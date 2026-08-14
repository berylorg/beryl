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

pub use beryl_app::{BerylWorkspacePersistence, WorkspacePersistenceError, WorkspaceUiState};
use beryl_model::workspace::BerylWorkspaceId;

use workspace_persistence_worker::{
    WorkspacePersistenceCommandForTest as Command,
    WorkspacePersistenceCommandKindForTest::{Flush, Write},
    collect_workspace_persistence_batch_kinds_for_test,
    collect_workspace_persistence_batches_for_test, spawn_workspace_persistence_worker,
};

#[path = "../src/shell/workspace_persistence_worker.rs"]
mod workspace_persistence_worker;

#[test]
fn persistence_flush_stops_batch_before_later_work() {
    assert_eq!(
        collect_workspace_persistence_batch_kinds_for_test(Write, &[Flush, Write]),
        vec![Write, Flush]
    );
    assert_eq!(
        collect_workspace_persistence_batch_kinds_for_test(Flush, &[Write]),
        vec![Flush]
    );
}

#[test]
fn repeated_workspace_state_and_ui_state_commands_coalesce_before_flush() {
    assert_eq!(
        collect_workspace_persistence_batches_for_test(&[
            Command::WorkspaceState {
                workspace_id: "workspace".to_string(),
                touch_manifest: false,
            },
            Command::WorkspaceState {
                workspace_id: "workspace".to_string(),
                touch_manifest: true,
            },
            Command::WorkspaceUiState {
                workspace_id: "workspace".to_string(),
                panel_height_px: 120.0,
            },
            Command::WorkspaceUiState {
                workspace_id: "workspace".to_string(),
                panel_height_px: 240.0,
            },
            Command::Flush,
        ]),
        vec![vec![
            Command::WorkspaceState {
                workspace_id: "workspace".to_string(),
                touch_manifest: true,
            },
            Command::WorkspaceUiState {
                workspace_id: "workspace".to_string(),
                panel_height_px: 240.0,
            },
            Command::Flush,
        ]]
    );
}

#[test]
fn flush_boundary_keeps_later_coalesced_work_out_of_flush_batch() {
    assert_eq!(
        collect_workspace_persistence_batches_for_test(&[
            Command::WorkspaceState {
                workspace_id: "workspace".to_string(),
                touch_manifest: false,
            },
            Command::Flush,
            Command::WorkspaceState {
                workspace_id: "workspace".to_string(),
                touch_manifest: true,
            },
        ]),
        vec![
            vec![
                Command::WorkspaceState {
                    workspace_id: "workspace".to_string(),
                    touch_manifest: false,
                },
                Command::Flush,
            ],
            vec![Command::WorkspaceState {
                workspace_id: "workspace".to_string(),
                touch_manifest: true,
            }],
        ]
    );
}

#[test]
fn token_usage_snapshots_coalesce_by_workspace_thread() {
    assert_eq!(
        collect_workspace_persistence_batches_for_test(&[
            Command::TokenSnapshot {
                workspace_id: "workspace".to_string(),
                thread_id: "thread_a".to_string(),
                turn_id: "turn_1".to_string(),
            },
            Command::TokenSnapshot {
                workspace_id: "workspace".to_string(),
                thread_id: "thread_a".to_string(),
                turn_id: "turn_2".to_string(),
            },
            Command::TokenSnapshot {
                workspace_id: "workspace".to_string(),
                thread_id: "thread_b".to_string(),
                turn_id: "turn_3".to_string(),
            },
            Command::Flush,
        ]),
        vec![vec![
            Command::TokenSnapshot {
                workspace_id: "workspace".to_string(),
                thread_id: "thread_a".to_string(),
                turn_id: "turn_2".to_string(),
            },
            Command::TokenSnapshot {
                workspace_id: "workspace".to_string(),
                thread_id: "thread_b".to_string(),
                turn_id: "turn_3".to_string(),
            },
            Command::Flush,
        ]]
    );
}

#[test]
fn image_asset_marks_merge_duplicates_without_crossing_command_classes() {
    assert_eq!(
        collect_workspace_persistence_batches_for_test(&[
            Command::MarkReferenced {
                workspace_id: "workspace".to_string(),
                asset_ids: vec!["a".to_string(), "a".to_string(), "b".to_string()],
            },
            Command::MarkReferenced {
                workspace_id: "workspace".to_string(),
                asset_ids: vec!["b".to_string(), "c".to_string()],
            },
            Command::MarkUnreferenced {
                workspace_id: "workspace".to_string(),
                asset_ids: vec!["a".to_string(), "a".to_string()],
            },
            Command::MarkReferenced {
                workspace_id: "workspace".to_string(),
                asset_ids: vec!["c".to_string(), "d".to_string()],
            },
            Command::Flush,
        ]),
        vec![vec![
            Command::MarkReferenced {
                workspace_id: "workspace".to_string(),
                asset_ids: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            },
            Command::MarkUnreferenced {
                workspace_id: "workspace".to_string(),
                asset_ids: vec!["a".to_string()],
            },
            Command::MarkReferenced {
                workspace_id: "workspace".to_string(),
                asset_ids: vec!["c".to_string(), "d".to_string()],
            },
            Command::Flush,
        ]]
    );
}

#[test]
fn persistence_worker_writes_to_injected_root_when_environment_home_differs() {
    let env_home = unique_temp_dir("env-home");
    let injected_root = unique_temp_dir("injected-root");
    let workspace_id = BerylWorkspaceId::new("worker_injected_root").unwrap();

    with_environment_home(&env_home, || {
        let queue =
            spawn_workspace_persistence_worker(Ok(BerylWorkspacePersistence::new(&injected_root)));
        queue.save_workspace_ui_state(workspace_id.clone(), WorkspaceUiState::default());
        queue.flush().wait(Duration::from_secs(5)).unwrap();
    });

    assert!(
        injected_root
            .join("workspaces")
            .join(workspace_id.as_str())
            .join("workspace.redb")
            .exists()
    );
    assert!(!env_home.join(".beryl").exists());

    cleanup_temp_dir(injected_root);
    cleanup_temp_dir(env_home);
}

fn unique_temp_dir(label: &str) -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir(format!("beryl-workspace-persistence-worker-test-{label}-"))
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
