use super::*;

fn body() -> String {
    "short🍒\r\n".to_owned() + &"x".repeat(300_000) + "\r\n"
}

fn click_save(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    panel: &Entity<SyndicTranscriptPanel>,
) {
    let bounds = panel.read(app).code_save_geometry_for_test().unwrap();
    assert_eq!(bounds.size.height, px(32.0));
    code_interaction::press(native, app, bounds.center());
    code_interaction::release(native, app, bounds.center());
}

fn output_directory() -> std::path::PathBuf {
    let path = tempfile::Builder::new()
        .prefix("native-code-save-")
        .tempdir()
        .unwrap()
        .keep();
    if let Some(ledger) = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ledger)
            .unwrap();
        std::io::Write::write_all(&mut file, format!("{}\n", path.display()).as_bytes()).unwrap();
    }
    eprintln!("retained native Code Save output: {}", path.display());
    path
}

async fn completed(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| !panel.code_save_pending_for_test())
                .unwrap()
        },
        "Code Save must reach a terminal outcome",
    )
    .await;
}

#[test]
fn native_code_save_streams_complete_large_source_with_replacement_and_local_failures() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some(&format!("```text\n{}```", body()))),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx).await;
                let directory = output_directory();
                let path = directory.join("saved.txt");
                std::fs::write(&path, b"original").unwrap();
                with_native(window, cx, |native, app| {
                    let start = panel
                        .read(app)
                        .code_source_point_for_test(BODY_START + 1)
                        .unwrap();
                    let end = panel
                        .read(app)
                        .code_source_point_for_test(BODY_START + 4)
                        .unwrap();
                    drag(native, app, start, end);
                    assert!(
                        panel
                            .read(app)
                            .code_copy_unavailable_for_test()
                            .unwrap()
                            .contains("256 KiB")
                    );
                })
                .unwrap();
                let selection = panel
                    .read_with(cx, |panel, _| panel.code_selection_for_test())
                    .unwrap();
                for choice in [Ok(None), Err(())] {
                    let cancelled = choice.is_ok();
                    panel
                        .update(cx, |panel, _| panel.code_set_save_choice_for_test(choice))
                        .unwrap();
                    with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                    completed(window, &panel, cx).await;
                    if cancelled {
                        assert_eq!(
                            panel
                                .read_with(cx, |panel, _| panel.code_copy_feedback_for_test())
                                .unwrap(),
                            Some("")
                        );
                    }
                    assert_eq!(std::fs::read(&path).unwrap(), b"original");
                    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                    assert_eq!(
                        panel
                            .read_with(cx, |panel, _| panel.code_selection_for_test())
                            .unwrap(),
                        selection
                    );
                }
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_copy_feedback_for_test())
                        .unwrap(),
                    Some("Save unavailable: native picker failed")
                );
                for publication in [false, true] {
                    ready(window, &panel, cx).await;
                    panel
                        .update(cx, |panel, _| {
                            panel.code_set_save_choice_for_test(Ok(Some(gpui::SavePathChoice {
                                path: path.clone(),
                                replace_existing: true,
                            })));
                            panel.code_set_save_fault_for_test(publication);
                        })
                        .unwrap();
                    with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                    completed(window, &panel, cx).await;
                    assert_eq!(std::fs::read(&path).unwrap(), b"original");
                    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                    assert!(
                        panel
                            .read_with(cx, |panel, _| panel
                                .code_copy_feedback_for_test()
                                .unwrap()
                                .contains("failed"))
                            .unwrap()
                    );
                    assert_eq!(
                        panel
                            .read_with(cx, |panel, _| panel.code_selection_for_test())
                            .unwrap(),
                        selection
                    );
                }
                ready(window, &panel, cx).await;
                panel
                    .update(cx, |panel, _| {
                        panel.code_set_save_choice_for_test(Ok(Some(gpui::SavePathChoice {
                            path: path.clone(),
                            replace_existing: true,
                        })))
                    })
                    .unwrap();
                with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                completed(window, &panel, cx).await;
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_copy_feedback_for_test())
                        .unwrap(),
                    Some("Saved")
                );
                assert_eq!(std::fs::read(&path).unwrap(), body().as_bytes());
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_selection_for_test())
                        .unwrap(),
                    selection
                );
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_source_action_charge_for_test())
                        .unwrap(),
                    4096
                );
                assert!(
                    !panel
                        .read_with(cx, |panel, _| panel.code_save_pending_for_test())
                        .unwrap()
                );
                ready(window, &panel, cx).await;
                let revision = panel
                    .read_with(cx, |panel, _| {
                        panel.code_provider_revision_for_test().unwrap()
                    })
                    .unwrap();
                let gate = window
                    .update(cx, |root, _, _| root.test_hold_indexed_code_source_read(revision))
                    .unwrap();
                panel
                    .update(cx, |panel, _| {
                        panel.code_set_save_choice_for_test(Ok(Some(gpui::SavePathChoice {
                            path: path.clone(),
                            replace_existing: true,
                        })))
                    })
                    .unwrap();
                with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                wait_until(
                    window,
                    cx,
                    |_| gate.entered(),
                    "Save must stage before the held Source query returns",
                )
                .await;
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
                panel
                    .update(cx, |panel, cx| {
                        let demand = panel.resource_demand().unwrap();
                        panel.reject_resource_demand(demand);
                        cx.notify();
                    })
                    .unwrap();
                gate.release();
                completed(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |_| std::fs::read_dir(&directory).unwrap().count() == 1,
                    "Source refusal must remove its owned staging file",
                )
                .await;
                assert_eq!(std::fs::read(&path).unwrap(), body().as_bytes());
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_selection_for_test())
                        .unwrap(),
                    selection
                );
                ready(window, &panel, cx).await;
                panel
                    .update(cx, |panel, _| {
                        panel.code_set_save_choice_for_test(Ok(Some(gpui::SavePathChoice {
                            path: path.clone(),
                            replace_existing: true,
                        })))
                    })
                    .unwrap();
                with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                completed(window, &panel, cx).await;
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_copy_feedback_for_test())
                        .unwrap(),
                    Some("Saved")
                );
                assert_eq!(std::fs::read(&path).unwrap(), body().as_bytes());
                ready(window, &panel, cx).await;
                let gate = window
                    .update(cx, |root, _, _| root.test_hold_indexed_code_source_read(revision))
                    .unwrap();
                let retired_path = directory.join("retired.txt");
                panel
                    .update(cx, |panel, _| {
                        panel.code_set_save_choice_for_test(Ok(Some(gpui::SavePathChoice {
                            path: retired_path.clone(),
                            replace_existing: false,
                        })))
                    })
                    .unwrap();
                with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                wait_until(
                    window,
                    cx,
                    |_| gate.entered(),
                    "Retiring Save must own a stage and held Source read",
                )
                .await;
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
                window
                    .update(cx, |root, window, cx| {
                        root.test_suspend_code_saves_for_close(window, cx);
                        assert!(!root.running_thread_reads_drained());
                        assert!(!root.release_suspended_running_thread_sources());
                    })
                    .unwrap();
                gate.release();
                wait_until(
                    window,
                    cx,
                    |_| std::fs::read_dir(&directory).unwrap().count() == 1,
                    "Host retirement must remove incomplete Save staging",
                )
                .await;
                assert!(!retired_path.exists());
                wait_until(window, cx, |cx| window.read_with(cx, |root, _| root.running_thread_reads_drained()).unwrap(), "Root disposal must wait for Source read and Save staging cleanup").await;
                assert_eq!(std::fs::read(&path).unwrap(), body().as_bytes());
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_code_save_empty_source_and_late_picker_retirement_preserve_output() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("```text\n```")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.code_save_geometry_for_test().is_some()
                                    && panel.code_commands_current_for_test()
                                    && panel
                                        .diagnostic_snapshot()
                                        .code_panels
                                        .first()
                                        .is_some_and(|code| code.layout_state == "ready")
                            })
                            .unwrap()
                    },
                    "empty Code must mount a ready Save command",
                )
                .await;
                let directory = output_directory();
                let path = directory.join("empty.txt");
                panel
                    .update(cx, |panel, _| {
                        panel.code_set_save_choice_for_test(Ok(Some(gpui::SavePathChoice {
                            path: path.clone(),
                            replace_existing: false,
                        })))
                    })
                    .unwrap();
                with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                completed(window, &panel, cx).await;
                assert_eq!(std::fs::read(&path).unwrap(), b"");
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_copy_feedback_for_test())
                        .unwrap(),
                    Some("Saved")
                );
                for publish in [false, true] {
                    let held_path = directory.join(if publish { "elected.txt" } else { "cancelled.txt" });
                    let gate = panel.update(cx, |panel, _| {
                        panel.code_set_save_choice_for_test(Ok(Some(gpui::SavePathChoice { path: held_path.clone(), replace_existing: false })));
                        panel.code_hold_save_writer_for_test(publish)
                    }).unwrap();
                    with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                    wait_until(window, cx, |_| gate.entered(), "Save must reach the held filesystem operation").await;
                    window.update(cx, |root, window, cx| {
                        root.test_suspend_code_saves_for_close(window, cx);
                        assert!(!root.running_thread_reads_drained());
                        assert!(!root.release_suspended_running_thread_sources());
                    }).unwrap();
                    with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                    assert!(!panel.read_with(cx, |panel, _| panel.code_save_pending_for_test()).unwrap());
                    gate.release();
                    wait_until(window, cx, |cx| window.read_with(cx, |root, _| root.running_thread_reads_drained()).unwrap(), "Root drain must await cleanup or elected publication").await;
                    assert_eq!(held_path.exists(), publish);
                    if publish { assert_eq!(std::fs::read(&held_path).unwrap(), b""); }
                    window.update(cx, |root, window, cx| root.test_resume_code_saves_after_close(window, cx)).unwrap();
                    wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| panel.code_save_unavailable_for_test().is_none()).unwrap(), "Save must observe terminal lease release before re-enabling").await;
                }
                let late = panel
                    .update(cx, |panel, _| panel.code_defer_save_choice_for_test())
                    .unwrap();
                with_native(window, cx, |native, app| click_save(native, app, &panel)).unwrap();
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel.code_save_pending_for_test())
                        .unwrap()
                );
                window
                    .update(cx, |root, window, cx| {
                        root.test_suspend_code_saves_for_close(window, cx);
                        assert!(!root.running_thread_reads_drained());
                        assert!(!root.release_suspended_running_thread_sources());
                    })
                    .unwrap();
                late.send(Ok(Some(gpui::SavePathChoice {
                    path: directory.join("retired.txt"),
                    replace_existing: false,
                })))
                .unwrap();
                wait_until(window, cx, |cx| window.read_with(cx, |root, _| root.running_thread_reads_drained()).unwrap(), "Root drain must retain a cancelled native picker until completion").await;
                assert!(!directory.join("retired.txt").exists());
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
