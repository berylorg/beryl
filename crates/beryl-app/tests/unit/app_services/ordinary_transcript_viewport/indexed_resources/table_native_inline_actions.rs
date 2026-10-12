use super::*;

#[test]
fn native_inline_table_save_honors_confirmed_replacement_and_exact_source() {
    run_table_fixture(
        1,
        0,
        |path| seed_history_text(path, 1, Some(&exact_inline_table())),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx).await;
                let directory = output_directory();
                let destination = directory.join("inline.md");
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
                        "inline Save must reach terminal publication outcome",
                    )
                    .await;
                    assert_eq!(
                        std::fs::read(&destination).unwrap(),
                        if replacing {
                            exact_inline_table().into_bytes()
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
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel.resource_demand().is_none())
                        .unwrap()
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_inline_table_fallback_refuses_rectangle_and_preserves_raw_actions() {
    let raw = "| x\\|y | **z** | | surplus |\r\n";
    let table = format!(
        "| first | second | third |\r\n| :----- | -----: | :---: |\r\n| a | b | c |\r\n{raw}| last | | |\r\n"
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
                let captured = Rc::new(RefCell::new(Vec::<String>::new()));
                let sink = captured.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.set_table_copy_writer_for_test(move |item, _, _| {
                            sink.borrow_mut().push(item.text().unwrap());
                            Ok(())
                        })
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].fallback_rows,
                        1
                    );
                    click(native, app, cell_point(panel.read(app), 1, 0), false);
                    click(native, app, cell_point(panel.read(app), 3, 2), true);
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].selection,
                        Some(((1, 0), (1, 0)))
                    );
                    let copy = panel
                        .read(app)
                        .table_fallback_command_geometry_for_test(1)
                        .unwrap()
                        .0;
                    click(native, app, copy.center(), false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 1,
                    "inline fallback Copy must preserve authored row",
                )
                .await;
                assert_eq!(captured.borrow()[0], raw);
                let directory = output_directory();
                for (filename, fallback, expected_bytes) in [
                    ("raw-row.md", true, raw.as_bytes()),
                    ("whole.md", false, expected.as_bytes()),
                ] {
                    let destination = directory.join(filename);
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
                        let save = if fallback {
                            panel
                                .read(app)
                                .table_fallback_command_geometry_for_test(1)
                                .unwrap()
                                .1
                        } else {
                            panel.read(app).table_command_geometry_for_test().unwrap().1
                        };
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
                        "inline fallback and whole Save must finish",
                    )
                    .await;
                    assert_eq!(std::fs::read(&destination).unwrap(), expected_bytes);
                    ready(window, &panel, cx).await;
                }
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
