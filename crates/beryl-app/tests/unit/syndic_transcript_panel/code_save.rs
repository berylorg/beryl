use super::super::super::super::code_panel::{
    save::{CodeSave, SAVE_STATE_BYTES, SaveFault, SaveOperation, SaveResult, SaveWriterOptions},
    source_actions::CodeSourceActions,
};
use super::*;

fn open(path: std::path::PathBuf) -> SaveOperation {
    SaveOperation::Open(gpui::SavePathChoice {
        path,
        replace_existing: true,
    })
}

fn directory() -> std::path::PathBuf {
    let path = tempfile::Builder::new()
        .prefix("code-save-writer-")
        .tempdir()
        .unwrap()
        .keep();
    if let Some(ledger) = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER") {
        let mut ledger = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ledger)
            .unwrap();
        std::io::Write::write_all(&mut ledger, format!("{}\n", path.display()).as_bytes()).unwrap();
    }
    eprintln!("retained Code Save fixture: {}", path.display());
    path
}

fn request(
    save: &mut CodeSave,
    executor: &gpui::BackgroundExecutor,
    operation: SaveOperation,
) -> SaveResult {
    executor.block_test(save.work(operation).unwrap()).unwrap()
}

#[gpui::test]
fn code_save_writer_publishes_exact_complete_bytes_and_confirmed_replacement(
    cx: &mut TestAppContext,
) {
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        let raw = "aa".to_owned() + &"🍒\r\n".repeat(60_000) + "tail\r\n\0";
        let response = code_line_response(1, raw.len() as u64);
        let mut actions = CodeSourceActions::default();
        actions.observe(&viewport.owner, viewport.revision, Some(&response));
        actions.select(2, false);
        actions.select(6, true);
        let selection = actions.selection;
        assert!(actions.copy_budget(false).is_none());
        let mut save = CodeSave::new(&mut actions, &viewport.owner).unwrap();
        let _ = save.attach_writer(&executor, SaveWriterOptions::default());
        let path = directory().join("output.txt");
        std::fs::write(&path, b"original").unwrap();
        assert_eq!(
            request(&mut save, &executor, open(path.clone())),
            SaveResult::Opened
        );
        for bytes in raw.as_bytes().chunks(8192) {
            assert_eq!(
                request(&mut save, &executor, SaveOperation::Write(bytes.to_vec())),
                SaveResult::Wrote(bytes.len())
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"original");
        }
        assert_eq!(
            request(&mut save, &executor, SaveOperation::Prepare),
            SaveResult::Prepared
        );
        assert!(save.current(&actions, &viewport.owner));
        assert!(save.elect());
        assert!(!save.elect());
        assert_eq!(
            request(&mut save, &executor, SaveOperation::Publish),
            SaveResult::Published
        );
        assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
        assert_eq!(actions.selection, selection);
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    });
}

#[gpui::test]
fn code_save_writer_empty_source_creates_empty_destination(cx: &mut TestAppContext) {
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        let mut actions = CodeSourceActions::default();
        actions.observe(
            &viewport.owner,
            viewport.revision,
            Some(&code_line_response(1, 0)),
        );
        let mut save = CodeSave::new(&mut actions, &viewport.owner).unwrap();
        let _ = save.attach_writer(&executor, SaveWriterOptions::default());
        let path = directory().join("empty.txt");
        assert_eq!(
            request(&mut save, &executor, open(path.clone())),
            SaveResult::Opened
        );
        assert_eq!(
            request(&mut save, &executor, SaveOperation::Prepare),
            SaveResult::Prepared
        );
        assert!(save.elect());
        assert_eq!(
            request(&mut save, &executor, SaveOperation::Publish),
            SaveResult::Published
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"");
    });
}

#[gpui::test]
fn code_save_writer_retirement_before_election_discards_stage_and_preserves_destination(
    cx: &mut TestAppContext,
) {
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        for existing in [false, true] {
            let mut actions = CodeSourceActions::default();
            actions.observe(
                &viewport.owner,
                viewport.revision,
                Some(&code_line_response(1, 10)),
            );
            let mut save = CodeSave::new(&mut actions, &viewport.owner).unwrap();
            let _ = save.attach_writer(&executor, SaveWriterOptions::default());
            let path = directory().join("cancelled.txt");
            if existing {
                std::fs::write(&path, b"original").unwrap();
            }
            assert_eq!(
                request(&mut save, &executor, open(path.clone())),
                SaveResult::Opened
            );
            assert_eq!(
                request(
                    &mut save,
                    &executor,
                    SaveOperation::Write(b"partial".to_vec())
                ),
                SaveResult::Wrote(7)
            );
            drop(save);
            executor.run_until_parked();
            if existing {
                assert_eq!(std::fs::read(&path).unwrap(), b"original");
            } else {
                assert!(!path.exists());
            }
            assert_eq!(
                std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
                usize::from(existing)
            );
        }
    });
}

#[gpui::test]
fn code_save_writer_elected_publication_survives_owner_disposal_once(cx: &mut TestAppContext) {
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        let mut actions = CodeSourceActions::default();
        actions.observe(
            &viewport.owner,
            viewport.revision,
            Some(&code_line_response(1, 4)),
        );
        let mut save = CodeSave::new(&mut actions, &viewport.owner).unwrap();
        let _ = save.attach_writer(&executor, SaveWriterOptions::default());
        let path = directory().join("elected.txt");
        request(&mut save, &executor, open(path.clone()));
        request(&mut save, &executor, SaveOperation::Write(b"done".to_vec()));
        request(&mut save, &executor, SaveOperation::Prepare);
        assert!(save.elect());
        let completion = save.work(SaveOperation::Publish).unwrap();
        drop(save);
        drop(actions);
        assert_eq!(
            executor.block_test(completion).unwrap(),
            SaveResult::Published
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"done");
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    });
}

#[gpui::test]
fn code_save_writer_write_and_publication_failures_preserve_destination_and_clean_stage(
    cx: &mut TestAppContext,
) {
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        for fault in [SaveFault::Write, SaveFault::Publish] {
            for existing in [false, true] {
                let mut actions = CodeSourceActions::default();
                actions.observe(
                    &viewport.owner,
                    viewport.revision,
                    Some(&code_line_response(1, 4)),
                );
                let mut save = CodeSave::new(&mut actions, &viewport.owner).unwrap();
                let _ = save.attach_writer(&executor, SaveWriterOptions { fault: Some(fault), ..Default::default() });
                let path = directory().join("failed.txt");
                if existing {
                    std::fs::write(&path, b"original").unwrap();
                }
                request(&mut save, &executor, open(path.clone()));
                let result = request(&mut save, &executor, SaveOperation::Write(b"done".to_vec()));
                let result = if matches!(fault, SaveFault::Publish) {
                    request(&mut save, &executor, SaveOperation::Prepare);
                    assert!(save.elect());
                    request(&mut save, &executor, SaveOperation::Publish)
                } else {
                    result
                };
                assert!(matches!(result, SaveResult::Failed(_)));
                drop(save);
                executor.run_until_parked();
                if existing {
                    assert_eq!(std::fs::read(&path).unwrap(), b"original");
                } else {
                    assert!(!path.exists());
                }
                assert_eq!(
                    std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
                    usize::from(existing)
                );
            }
        }
    });
}

#[gpui::test]
fn code_save_custody_and_refusal_keep_selection_with_bounded_action_state(cx: &mut TestAppContext) {
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        let mut actions = CodeSourceActions::default();
        let mut owner = viewport.owner.clone();
        actions.observe(
            &owner,
            viewport.revision,
            Some(&code_line_response(1, 300_000)),
        );
        actions.select(20, false);
        actions.select(80, true);
        let selected = actions.selection;
        let mut save = CodeSave::new(&mut actions, &owner).unwrap();
        owner.presentation_revision += 1;
        assert!(save.current(&actions, &owner));
        owner.activation_revision += 1;
        assert!(!save.current(&actions, &owner));
        owner.activation_revision -= 1;
        let _ = save.attach_writer(&executor, SaveWriterOptions::default());
        save.busy = false;
        actions.save = Some(save);
        assert_eq!(actions.charge(), 4096 + SAVE_STATE_BYTES);
        owner.indexed = actions.query();
        assert!(
            matches!(owner.indexed.as_ref().unwrap().query, IndexedResourceQuery::Source { range, bytes: 16384 } if range.end - range.start == 8192)
        );
        assert!(actions.refuse_query(&owner));
        executor.run_until_parked();
        assert!(actions.save.is_none());
        assert_eq!(actions.selection, selected);
        assert_eq!(actions.charge(), 4096);
    });
}

#[gpui::test]
fn code_save_new_destination_collision_never_replaces_a_late_created_file(cx: &mut TestAppContext) {
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        let mut actions = CodeSourceActions::default();
        actions.observe(
            &viewport.owner,
            viewport.revision,
            Some(&code_line_response(1, 4)),
        );
        let mut save = CodeSave::new(&mut actions, &viewport.owner).unwrap();
        let _ = save.attach_writer(&executor, SaveWriterOptions::default());
        let path = directory().join("collision.txt");
        request(
            &mut save,
            &executor,
            SaveOperation::Open(gpui::SavePathChoice {
                path: path.clone(),
                replace_existing: false,
            }),
        );
        request(&mut save, &executor, SaveOperation::Write(b"done".to_vec()));
        request(&mut save, &executor, SaveOperation::Prepare);
        std::fs::write(&path, b"created after the native choice").unwrap();
        assert!(save.elect());
        assert_eq!(
            request(&mut save, &executor, SaveOperation::Publish),
            SaveResult::Failed("Save unavailable: publication failed")
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"created after the native choice"
        );
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    });
}

#[gpui::test]
fn code_save_staging_acquisition_failure_leaves_new_destination_absent(cx: &mut TestAppContext) {
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        let mut actions = CodeSourceActions::default();
        actions.observe(
            &viewport.owner,
            viewport.revision,
            Some(&code_line_response(1, 4)),
        );
        let mut save = CodeSave::new(&mut actions, &viewport.owner).unwrap();
        let _ = save.attach_writer(&executor, SaveWriterOptions::default());
        let directory = directory();
        let path = directory.join("absent-parent").join("output.txt");
        assert_eq!(
            request(&mut save, &executor, open(path.clone())),
            SaveResult::Failed("Save unavailable: staging file could not be created")
        );
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
    });
}

#[cfg(target_os = "windows")]
#[gpui::test]
fn code_save_cleanup_failure_is_a_terminal_explicit_outcome(cx: &mut TestAppContext) {
    use std::os::windows::fs::OpenOptionsExt;
    let executor = cx.background_executor.clone();
    with_code_layout(cx, |_, viewport, _| {
        let mut actions = CodeSourceActions::default();
        actions.observe(
            &viewport.owner,
            viewport.revision,
            Some(&code_line_response(1, 4)),
        );
        let mut save = CodeSave::new(&mut actions, &viewport.owner).unwrap();
        let _ = save.attach_writer(
            &executor,
            SaveWriterOptions {
                fault: Some(SaveFault::Write),
                ..Default::default()
            },
        );
        let directory = directory();
        let path = directory.join("output.txt");
        request(&mut save, &executor, open(path.clone()));
        let stage = std::fs::read_dir(&directory)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let blocker = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&stage)
            .unwrap();
        assert_eq!(
            request(&mut save, &executor, SaveOperation::Write(b"done".to_vec())),
            SaveResult::Failed(
                "Save unavailable: temporary output cleanup failed; a temporary file may remain"
            )
        );
        assert!(!path.exists());
        assert!(stage.exists());
        drop(blocker);
        std::fs::remove_file(stage).unwrap();
        assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
    });
}
