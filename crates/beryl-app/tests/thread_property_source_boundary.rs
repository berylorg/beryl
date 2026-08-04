use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn cas_thread_name_is_not_thread_title_authority() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates_dir = manifest_dir
        .parent()
        .expect("app crate should be under workspace crates");
    let backend_src_dir = crates_dir.join("beryl-backend").join("src");
    let model_src_dir = crates_dir.join("beryl-model").join("src");
    let state_src_dir = crates_dir.join("beryl-state").join("src");
    let app_src_dir = manifest_dir.join("src");
    let backend_forbidden = [
        "thread/name/set",
        "ThreadNameSet",
        "ThreadSetNameParams",
        "set_thread_name(",
    ];
    let title_authority_forbidden = [
        "thread/name/set",
        "ThreadNameSet",
        "ThreadSetNameParams",
        "set_thread_name(",
        "apply_thread_name_update",
        "apply_authoritative_thread_name_update",
        "set_authoritative_thread_backend_name",
        "set_authoritative_backend_name",
        "set_thread_backend_name",
        "title_with_backend_name(",
        "backend_name",
        "ignored_backend_name_for_automatic_title",
        "ignores_backend_name_for_automatic_title",
        "with_ignored_backend_name_for_automatic_title",
    ];
    let obsolete_app_authority_forbidden = [
        "ThreadMetadataState",
        "CreateThreadMetadata",
        ".thread_metadata()",
        "derive_short_title_from_turn",
    ];
    let obsolete_state_authority_forbidden = [
        "beryl-thread-metadata",
        "ThreadMetadataState",
        "ThreadMetadataRecord",
        "CreateThreadMetadata",
        "SetGeneratedTitle",
        "UpdateTokenUsage",
        "ArchiveBranchDiscussion",
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&backend_src_dir) {
        let source = fs::read_to_string(&path).expect("backend source should be readable");
        for forbidden in backend_forbidden {
            if source.contains(forbidden) {
                offenders.push(format!(
                    "beryl-backend/{} contains {forbidden}",
                    display_test_path(&backend_src_dir, &path)
                ));
            }
        }
    }

    for (crate_label, src_dir) in [("beryl-app", app_src_dir), ("beryl-model", model_src_dir)] {
        for path in rust_files_under(&src_dir) {
            let source = fs::read_to_string(&path).expect("source should be readable");
            for forbidden in title_authority_forbidden {
                if source.contains(forbidden) {
                    offenders.push(format!(
                        "{crate_label}/{} contains {forbidden}",
                        display_test_path(&src_dir, &path)
                    ));
                }
            }
        }
    }

    let app_src_dir = manifest_dir.join("src");
    for path in rust_files_under(&app_src_dir) {
        let source = fs::read_to_string(&path).expect("app source should be readable");
        for forbidden in obsolete_app_authority_forbidden {
            if source.contains(forbidden) {
                offenders.push(format!(
                    "beryl-app/{} contains {forbidden}",
                    display_test_path(&app_src_dir, &path)
                ));
            }
        }
    }
    assert!(!app_src_dir.join("title_generation.rs").exists());

    for path in rust_files_under(&state_src_dir) {
        let source = fs::read_to_string(&path).expect("Beryl-state source should be readable");
        for forbidden in obsolete_state_authority_forbidden {
            if source.contains(forbidden) {
                offenders.push(format!(
                    "beryl-state/{} contains {forbidden}",
                    display_test_path(&state_src_dir, &path)
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "obsolete thread-title or metadata authority remains live: {offenders:?}"
    );
}

#[test]
fn scheduled_execution_reads_stay_on_syndic_authority() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scheduler_dir = manifest_dir
        .join("src")
        .join("cas_projection")
        .join("accepted_input_scheduler");
    let next_turn = fs::read_to_string(scheduler_dir.join("next_turn.rs"))
        .expect("next-turn scheduler source should be readable");
    let recovered_pending = fs::read_to_string(scheduler_dir.join("recovered_pending.rs"))
        .expect("recovered-pending scheduler source should be readable");
    let scheduler =
        fs::read_to_string(manifest_dir.join("src/cas_projection/accepted_input_scheduler.rs"))
            .expect("scheduler source should be readable");
    let service = fs::read_to_string(manifest_dir.join("src/cas_projection/service.rs"))
        .expect("projection service source should be readable");

    assert!(next_turn.contains(".thread_execution("));
    assert!(recovered_pending.contains(".thread_execution("));
    for source in [next_turn, recovered_pending, scheduler, service] {
        assert!(!source.contains("ThreadMetadataState"));
        assert!(!source.contains("thread_metadata"));
    }
}

fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("source directory should be readable") {
            let entry = entry.expect("source directory entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    files
}

fn display_test_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
