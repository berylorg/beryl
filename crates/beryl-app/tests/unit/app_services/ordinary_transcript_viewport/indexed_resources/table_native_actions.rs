use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn native_table_full_selection_copy_cap_preserves_rectangle_and_save_streams_full_source() {
    let row = format!("| {} | second |\r\n", "x".repeat(7000));
    let table = format!("| first | second |\r\n|---|---:|\r\n{}", row.repeat(10));
    assert!(table.len() > 65_536);
    assert!(
        table
            .lines()
            .all(|line| line.len() < syndic_storage::MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES)
    );
    let expected = table.clone();
    run_table_fixture(
        1,
        0,
        move |path| seed_history_text(path, 1, Some(&table)),
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx).await;
                let writes = Rc::new(Cell::new(0));
                let sink = writes.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.set_table_copy_writer_for_test(move |_, _, _| {
                            sink.set(sink.get() + 1);
                            Ok(())
                        })
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    click(native, app, cell_point(panel.read(app), 0, 0), false);
                    click(native, app, cell_point(panel.read(app), 10, 1), true);
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].selection,
                        Some(((0, 0), (10, 1)))
                    );
                    let copy = panel.read(app).table_command_geometry_for_test().unwrap().0;
                    click(native, app, copy.center(), false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                !panel.table_copy_pending_for_test()
                                    && panel.table_feedback_for_test().contains("64 KiB")
                            })
                            .unwrap()
                    },
                    "whole Table Copy must refuse before checked clipboard writer",
                )
                .await;
                assert_eq!(writes.get(), 0);
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.diagnostic_snapshot().table_panels[0]
                            .selection)
                        .unwrap(),
                    Some(((0, 0), (10, 1)))
                );
                let directory = output_directory();
                let destination = directory.join("large-table.md");
                let (send, receive) = futures_channel::oneshot::channel();
                send.send(Ok(Some(gpui::SavePathChoice {
                    path: destination.clone(),
                    replace_existing: false,
                })))
                .unwrap();
                panel
                    .update(cx, |panel, _| panel.table_save_choice_for_test(receive))
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let save = panel.read(app).table_command_geometry_for_test().unwrap().1;
                    click(native, app, save.center(), false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| !panel.table_save_pending_for_test())
                            .unwrap()
                    },
                    "whole Table Save must bypass clipboard size limit through bounded pages",
                )
                .await;
                assert_eq!(std::fs::read(&destination).unwrap(), expected.as_bytes());
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                assert_eq!(writes.get(), 0);
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.diagnostic_snapshot().table_panels[0]
                            .selection)
                        .unwrap(),
                    Some(((0, 0), (10, 1)))
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_table_fallback_row_copy_and_save_use_full_authored_surplus_row() {
    let raw = "| x\\|y | **z** | | surplus |\r\n";
    let table = format!(
        "| first | second | third |\r\n| :----- | -----: | :---: |\r\n{raw}{}",
        resource_padding_rows()
    );
    run_table_fixture(
        1,
        0,
        move |path| seed_history_text(path, 1, Some(&table)),
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx).await;
                let captured = Rc::new(RefCell::new(Vec::<String>::new()));
                let sink = captured.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.set_table_copy_writer_for_test(move |item, _, _| {
                            assert!(sink.borrow().len() < 2);
                            sink.borrow_mut().push(item.text().unwrap());
                            Ok(())
                        })
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let diagnostics = panel.read(app).diagnostic_snapshot();
                    assert_eq!(diagnostics.table_panels[0].fallback_rows, 1);
                    assert_eq!(diagnostics.table_panels[0].selection, None);
                    let (copy, save) = panel
                        .read(app)
                        .table_fallback_command_geometry_for_test(0)
                        .unwrap();
                    let viewport = panel.read(app).table_viewport_for_test().unwrap().1;
                    assert_eq!(copy.intersect(&viewport), copy);
                    assert_eq!(save.intersect(&viewport), save);
                    click(native, app, copy.center(), false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 1,
                    "fallback row Copy must reach exact-source checked writer",
                )
                .await;
                assert_eq!(captured.borrow()[0], raw);
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel.diagnostic_snapshot().table_panels[0]
                            .selection
                            .is_none())
                        .unwrap()
                );
                ready(window, &panel, cx).await;
                let directory = output_directory();
                let destination = directory.join("raw-row.md");
                let (send, receive) = futures_channel::oneshot::channel();
                send.send(Ok(Some(gpui::SavePathChoice {
                    path: destination.clone(),
                    replace_existing: false,
                })))
                .unwrap();
                panel
                    .update(cx, |panel, _| panel.table_save_choice_for_test(receive))
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let save = panel
                        .read(app)
                        .table_fallback_command_geometry_for_test(0)
                        .unwrap()
                        .1;
                    click(native, app, save.center(), false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| !panel.table_save_pending_for_test())
                            .unwrap()
                    },
                    "fallback row Save must stream exact authored row",
                )
                .await;
                assert_eq!(std::fs::read(&destination).unwrap(), raw.as_bytes());
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel.diagnostic_snapshot().table_panels[0]
                            .selection
                            .is_none())
                        .unwrap()
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_table_keyboard_copy_preserves_cell_markdown_and_distinct_second_press_text() {
    run_table_fixture(
        1,
        0,
        |path| seed_history_text(path, 1, Some(&small_table())),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx).await;
                let captured = Rc::new(RefCell::new(Vec::<String>::new()));
                let refuse = Rc::new(Cell::new(false));
                let sink = captured.clone();
                let refusal = refuse.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.set_table_copy_writer_for_test(move |item, _, _| {
                            assert!(sink.borrow().len() < 12);
                            sink.borrow_mut().push(item.text().unwrap());
                            if refusal.get() {
                                Err(gpui::ClipboardError::Write(
                                    gpui::ClipboardRepresentation::Text,
                                ))
                            } else {
                                Ok(())
                            }
                        })
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    click(native, app, cell_point(panel.read(app), 1, 0), false);
                    click(native, app, cell_point(panel.read(app), 3, 2), true);
                    key(native, app, "ctrl-c", false);
                    key(native, app, "ctrl-c", true);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 1,
                    "first Table Copy must reach checked writer",
                )
                .await;
                assert_eq!(
                    captured.borrow()[0],
                    "| **bold\\|** | | `tail*` |\n| | | |\n| last | | |"
                );
                with_native(window, cx, |native, app| {
                    click(native, app, cell_point(panel.read(app), 3, 2), true);
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].selection,
                        Some(((1, 0), (3, 2)))
                    );
                    key(native, app, "ctrl-c", false);
                    assert!(panel.read(app).table_copy_pending_for_test());
                    key(native, app, "ctrl-c", true);
                    key(native, app, "ctrl-c", false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 2,
                    "distinct second Table Copy must reach checked writer",
                )
                .await;
                assert_eq!(captured.borrow()[1], "**bold|**\t\ttail*\nlast");
                refuse.set(true);
                with_native(window, cx, |native, app| {
                    click(native, app, cell_point(panel.read(app), 1, 0), false);
                    key(native, app, "ctrl-c", false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 3,
                    "refused Table Copy must terminate at checked writer",
                )
                .await;
                panel
                    .read_with(cx, |panel, _| {
                        assert_eq!(
                            panel.diagnostic_snapshot().table_panels[0].selection,
                            Some(((1, 0), (1, 0)))
                        );
                        assert!(!panel.table_feedback_for_test().is_empty());
                    })
                    .unwrap();
                refuse.set(false);
                with_native(window, cx, |native, app| {
                    click(native, app, cell_point(panel.read(app), 0, 1), false);
                    click(native, app, cell_point(panel.read(app), 1, 1), true);
                    key(native, app, "ctrl-c", false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 4,
                    "selected header Table Copy must include selected delimiter only",
                )
                .await;
                assert_eq!(captured.borrow()[3], "| head1 |\n|---:|\n| |");
                with_native(window, cx, |native, app| {
                    click(native, app, cell_point(panel.read(app), 0, 0), false);
                    click(native, app, cell_point(panel.read(app), 6, 2), true);
                    key(native, app, "ctrl-c", false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 5,
                    "whole Table Copy must reproduce exact raw source",
                )
                .await;
                assert_eq!(captured.borrow()[4], small_table());
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_table_save_control_streams_beyond_resident_window_and_honors_replacement_choice() {
    run_table_fixture(
        1,
        0,
        |path| seed_history_text(path, 1, Some(&large_table())),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx).await;
                let directory = output_directory();
                let destination = directory.join("saved.md");
                std::fs::write(&destination, b"original destination").unwrap();
                for choice in [
                    None,
                    Some(gpui::SavePathChoice {
                        path: destination.clone(),
                        replace_existing: false,
                    }),
                    Some(gpui::SavePathChoice {
                        path: destination.clone(),
                        replace_existing: true,
                    }),
                ] {
                    let replacing = choice
                        .as_ref()
                        .is_some_and(|choice| choice.replace_existing);
                    let (send, receive) = futures_channel::oneshot::channel();
                    send.send(Ok(choice)).unwrap();
                    panel
                        .update(cx, |panel, _| panel.table_save_choice_for_test(receive))
                        .unwrap();
                    with_native(window, cx, |native, app| {
                        let bounds = panel.read(app).table_command_geometry_for_test().unwrap().1;
                        click(native, app, bounds.center(), false);
                    })
                    .unwrap();
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| !panel.table_save_pending_for_test())
                                .unwrap()
                        },
                        "Table Save must reach terminal custody outcome",
                    )
                    .await;
                    assert_eq!(
                        std::fs::read(&destination).unwrap(),
                        if replacing {
                            large_table().into_bytes()
                        } else {
                            b"original destination".to_vec()
                        }
                    );
                    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                    if replacing {
                        assert_eq!(
                            panel
                                .read_with(cx, |panel, _| panel.table_feedback_for_test())
                                .unwrap(),
                            "Saved"
                        );
                    }
                    ready(window, &panel, cx).await;
                }
                assert!(large_table().len() > 8192);
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel.diagnostic_snapshot().table_panels[0]
                            .cells
                            <= 128)
                        .unwrap()
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
