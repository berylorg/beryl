use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[path = "table_native_inline_actions.rs"]
mod actions;

fn maximum_inline_table() -> String {
    let mut source = String::from("|h0|h1|h2|h3|h4|h5|h6|h7|h8|h9|h10|h11|\r\n");
    source.push_str("|:---|---:|:---:|---|---|---|---|---|---|---|---|---|\r\n");
    for row in 0..32 {
        source.push('|');
        for column in 0..12 {
            source.push_str(&format!("r{row}c{column}|"));
        }
        source.push_str("\r\n");
    }
    assert!(source.len() <= 8192);
    source
}

fn exact_inline_table() -> String {
    "| head0 | head1 | head2 |\r\n| :----- | -----: | :---: |\r\n| **bold\\|** | | `tail*` |\r\n| | | |\r\n| last | |\r\n".to_owned()
}

#[test]
fn native_inline_table_resize_and_font_preserve_outer_identity() {
    table_resize_and_font(maximum_inline_table, 396, true);
}

#[test]
fn native_inline_table_selects_offscreen_cells_without_resource_demand() {
    run_table_fixture(
        1,
        0,
        |path| seed_history_text(path, 1, Some(&maximum_inline_table())),
        |owner, _, cx| {
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
                let identity = panel
                    .read_with(cx, |panel, _| {
                        let snapshot = panel.snapshot();
                        assert!(snapshot.resources.metadata.is_empty());
                        assert!(snapshot.resources.slices.is_empty());
                        assert!(panel.resource_demand().is_none());
                        assert_eq!(
                            panel.inline_table_source_for_test(),
                            Some(maximum_inline_table().as_str())
                        );
                        let diagnostics = panel.diagnostic_snapshot();
                        let table = &diagnostics.table_panels[0];
                        assert_eq!(table.rows, 32);
                        assert_eq!(table.columns, 12);
                        assert!(table.realized_rows.end - table.realized_rows.start <= 15);
                        assert!(table.realized_columns.end - table.realized_columns.start <= 8);
                        table.id
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let position = cell_point(panel.read(app), 1, 0);
                    wheel(native, app, position, -10.0, -10.0);
                    assert!(
                        !panel.read(app).diagnostic_snapshot().table_panels[0].selected_for_wheel
                    );
                    click(native, app, position, false);
                    let outer = panel
                        .read(app)
                        .measured_frame_for_test()
                        .unwrap()
                        .manual_scroll_total_px;
                    let (body, header, _, _) = table_geometry(panel.read(app)).unwrap();
                    let command = panel.read(app).table_command_geometry_for_test().unwrap().0;
                    for position in [body.center(), header.center(), command.center()] {
                        let before = panel.read(app).diagnostic_snapshot().table_panels[0].clone();
                        wheel(native, app, position, -10.25, -12.5);
                        let after = panel.read(app).diagnostic_snapshot().table_panels[0].clone();
                        assert_eq!(
                            after.offsets,
                            (before.offsets.0 + 10.25, before.offsets.1 + 12.5)
                        );
                        assert_eq!(after.selection, before.selection);
                        assert_eq!(
                            panel
                                .read(app)
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            outer
                        );
                        wheel(native, app, position, 10.25, 12.5);
                    }
                    let start = cell_point(panel.read(app), 1, 0);
                    let end = cell_point(panel.read(app), 3, 1);
                    native.dispatch_input_for_test(
                        PlatformInput::MouseDown(gpui::MouseDownEvent {
                            position: start,
                            button: gpui::MouseButton::Left,
                            click_count: 1,
                            ..Default::default()
                        }),
                        app,
                    );
                    native.dispatch_input_for_test(
                        PlatformInput::MouseMove(gpui::MouseMoveEvent {
                            position: end,
                            pressed_button: Some(gpui::MouseButton::Left),
                            ..Default::default()
                        }),
                        app,
                    );
                    native.dispatch_input_for_test(
                        PlatformInput::MouseUp(gpui::MouseUpEvent {
                            position: end,
                            button: gpui::MouseButton::Left,
                            click_count: 1,
                            ..Default::default()
                        }),
                        app,
                    );
                    native.draw_and_present_for_test(app);
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].selection,
                        Some(((1, 0), (3, 1)))
                    );
                    key(native, app, "shift-right", false);
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].selection,
                        Some(((1, 0), (3, 2)))
                    );
                    click(native, app, cell_point(panel.read(app), 0, 0), false);
                    key(native, app, "shift-end", false);
                    for _ in 0..32 {
                        key(native, app, "shift-down", false);
                    }
                    let diagnostics = panel.read(app).diagnostic_snapshot();
                    let table = &diagnostics.table_panels[0];
                    assert_eq!(table.id, identity);
                    assert_eq!(table.selection, Some(((0, 0), (32, 11))));
                    assert_eq!(table.focused, Some((32, 11)));
                    assert!(table.offsets.0 > 0.0 && table.offsets.1 > 0.0);
                    assert!(table.realized_rows.end - table.realized_rows.start <= 15);
                    assert!(table.realized_columns.end - table.realized_columns.start <= 8);
                    assert!(panel.read(app).resource_demand().is_none());
                    key(native, app, "ctrl-c", false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 1,
                    "complete inline Table selection must copy all 396 source cells",
                )
                .await;
                assert_eq!(captured.borrow()[0], maximum_inline_table());
                retire_code_fixture(window, &panel, cx).await;
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .diagnostic_snapshot()
                            .table_panels
                            .is_empty())
                        .unwrap()
                );
            })
        },
    );
}

#[test]
fn native_inline_table_copy_preserves_empty_missing_cells_and_distinct_gesture() {
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
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .snapshot()
                            .resources
                            .metadata
                            .is_empty())
                        .unwrap()
                );
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .inline_table_source_for_test()
                            .map(str::to_owned))
                        .unwrap(),
                    Some(exact_inline_table())
                );
                let captured = Rc::new(RefCell::new(Vec::<String>::new()));
                let refuse = Rc::new(Cell::new(false));
                let sink = captured.clone();
                let refusal = refuse.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.set_table_copy_writer_for_test(move |item, _, _| {
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
                    "first inline Copy must publish Markdown",
                )
                .await;
                assert_eq!(
                    captured.borrow()[0],
                    "| **bold\\|** | | `tail*` |\n| | | |\n| last | ||"
                );
                with_native(window, cx, |native, app| {
                    click(native, app, cell_point(panel.read(app), 3, 2), true);
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
                    "distinct second inline Copy must publish stripped text",
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
                    "inline Copy refusal must complete",
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
                    "inline partial header Copy must retain only selected column",
                )
                .await;
                assert_eq!(captured.borrow()[3], "| head1 |\n|---:|\n| |");
                with_native(window, cx, |native, app| {
                    click(native, app, cell_point(panel.read(app), 0, 0), false);
                    click(native, app, cell_point(panel.read(app), 3, 2), true);
                    key(native, app, "ctrl-c", false);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| captured.borrow().len() == 5,
                    "inline whole Copy must preserve exact CRLF source",
                )
                .await;
                assert_eq!(captured.borrow()[4], exact_inline_table());
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
