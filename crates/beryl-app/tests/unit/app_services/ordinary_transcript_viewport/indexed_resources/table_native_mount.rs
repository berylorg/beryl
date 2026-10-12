use super::code_long_line::{
    retire_code_fixture, wait_until, wait_until_unminimized, wait_until_with_visibility,
    with_native,
};
use super::*;
use crate::syndic_transcript::SyndicTranscriptPanel;
use gpui::{AsyncApp, Entity, PlatformInput, point, px};
#[path = "table_native_support.rs"]
mod support;
use self::support::*;
#[path = "table_native_actions.rs"]
mod actions;
#[path = "table_native_inline.rs"]
mod inline;

#[test]
fn native_table_resize_and_active_font_remeasure_one_outer_record_coherently() {
    table_resize_and_font(large_table, 128, false);
}

fn table_resize_and_font(source: fn() -> String, cell_limit: usize, preserve_selection: bool) {
    run_table_fixture(
        1,
        0,
        move |path| seed_history_text(path, 1, Some(&source())),
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready_for_resize(window, &panel, cx).await;
                if preserve_selection {
                    with_native(window, cx, |native, app| {
                        click(native, app, cell_point(panel.read(app), 1, 0), false);
                        click(native, app, cell_point(panel.read(app), 2, 1), true);
                    })
                    .unwrap();
                }
                let original = window
                    .read_with(cx, |root, _| root.test_appearance())
                    .unwrap();
                let (id, before_width, before_height, before_track, anchor) = panel
                    .read_with(cx, |panel, _| {
                        let (bounds, _, height, track) = table_geometry(panel).unwrap();
                        (
                            panel.diagnostic_snapshot().table_panels[0].id,
                            bounds.size.width,
                            height,
                            track,
                            panel.measured_frame_for_test().unwrap().anchor.unwrap(),
                        )
                    })
                    .unwrap();
                with_native(window, cx, |native, _| {
                    native.resize(gpui::size(
                        native.viewport_size().width - px(120.0),
                        native.viewport_size().height,
                    ))
                })
                .unwrap();
                wait_until_unminimized(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                table_geometry(panel).is_some_and(|(bounds, _, _, _)| {
                                    bounds.size.width < before_width
                                }) && !panel.measurement_diagnostics().0
                            })
                            .unwrap()
                    },
                    "native width change must remeasure Table outer record",
                )
                .await;
                let prepared = {
                    let owner = owner.borrow();
                    table_appearance(owner.test_services().graph().unwrap().state())
                };
                let large = std::sync::Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                    original.number().checked_next().unwrap(),
                    prepared,
                    crate::theme_runtime::AppearancePublication::Durable,
                ));
                for (appearance, enlarged) in [(large, true), (original, false)] {
                    cx.update(|app| {
                        MainWindowShellRoot::test_apply_appearance(window, appearance, app)
                    })
                    .unwrap();
                    wait_until_unminimized(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| {
                                    table_geometry(panel).is_some_and(|(_, _, height, track)| {
                                        if enlarged {
                                            height > before_height && track > before_track
                                        } else {
                                            height == before_height && track == before_track
                                        }
                                    }) && !panel.measurement_diagnostics().0
                                        && panel.table_commands_current_for_test()
                                })
                                .unwrap()
                        },
                        "active Table font must settle demand, paint and measured outer geometry",
                    )
                    .await;
                    panel
                        .read_with(cx, |panel, _| {
                            let diagnostics = panel.diagnostic_snapshot();
                            let table = &diagnostics.table_panels[0];
                            let (bounds, header, height, _) = table_geometry(panel).unwrap();
                            assert_eq!(table.id, id);
                            if preserve_selection {
                                assert_eq!(table.selection, Some(((1, 0), (2, 1))));
                                assert_eq!(table.focused, Some((2, 1)));
                            }
                            assert_eq!(header.size.height, px(height));
                            assert!(bounds.size.height <= px(360.0));
                            assert!(table.cells <= cell_limit);
                            assert!(
                                (table.realized_rows.end - table.realized_rows.start) <= 15
                                    && (table.realized_columns.end - table.realized_columns.start)
                                        <= 8
                            );
                            assert!(panel.measurement_diagnostics().2 <= 32);
                            let current = panel.measured_frame_for_test().unwrap().anchor.unwrap();
                            assert_eq!(current.record_id, anchor.record_id);
                            assert_eq!(current.viewport_y_px, anchor.viewport_y_px);
                        })
                        .unwrap();
                }
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_table_mount_routes_both_axes_and_preserves_resident_cell_selection() {
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
                let (identity, outer) = panel
                    .read_with(cx, |panel, _| {
                        let diagnostics = panel.diagnostic_snapshot();
                        let table = &diagnostics.table_panels[0];
                        eprintln!("native Table initial diagnostics: {table:?}");
                        assert_eq!(table.rows, 24);
                        assert_eq!(table.columns, 12);
                        assert!(table.cells <= 128);
                        assert!((table.realized_rows.end - table.realized_rows.start) <= 15);
                        assert!((table.realized_columns.end - table.realized_columns.start) <= 8);
                        assert!(table.horizontal_scrollbar && table.vertical_scrollbar);
                        assert!(!table.selected_for_wheel);
                        assert!(panel.painted_glyphs_for_test() > 30);
                        (
                            table.id,
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                        )
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let position = cell_point(panel.read(app), 1, 0);
                    native.dispatch_input_for_test(
                        PlatformInput::MouseMove(gpui::MouseMoveEvent {
                            position,
                            ..Default::default()
                        }),
                        app,
                    );
                    wheel(native, app, position, 0.0, -20.0);
                    let diagnostics = panel.read(app).diagnostic_snapshot();
                    assert_eq!(diagnostics.table_panels[0].offsets.1, 0.0);
                    assert!(!diagnostics.table_panels[0].selected_for_wheel);
                    click(native, app, position, false);
                    assert!(native.focused(app).is_some());
                    let diagnostics = panel.read(app).diagnostic_snapshot();
                    assert_eq!(
                        diagnostics.table_panels[0].selection,
                        Some(((1, 0), (1, 0)))
                    );
                    assert!(diagnostics.table_panels[0].selected_for_wheel);
                    let (_, body_bounds, header_bounds, _, _) =
                        panel.read(app).table_viewport_for_test().unwrap();
                    let command_bounds =
                        panel.read(app).table_command_geometry_for_test().unwrap().0;
                    for header_position in [header_bounds.center(), command_bounds.center()] {
                        assert!(!body_bounds.contains(&header_position));
                        let keyboard_focus = native.focused(app).expect("Table keyboard focus");
                        let before = panel.read(app).diagnostic_snapshot().table_panels[0].clone();
                        let outer_before = panel
                            .read(app)
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px;
                        wheel(native, app, header_position, -10.25, -12.5);
                        let after = panel.read(app).diagnostic_snapshot().table_panels[0].clone();
                        assert_eq!(
                            after.offsets,
                            (before.offsets.0 + 10.25, before.offsets.1 + 12.5)
                        );
                        assert_eq!(after.focused, before.focused);
                        assert_eq!(after.selection, before.selection);
                        assert!(after.selected_for_wheel);
                        assert!(
                            native
                                .focused(app)
                                .is_some_and(|focus| focus == keyboard_focus)
                        );
                        assert_eq!(
                            panel
                                .read(app)
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            outer_before,
                            "header wheel must not co-scroll the outer transcript"
                        );
                        wheel(native, app, header_position, 10.25, 12.5);
                        let restored =
                            panel.read(app).diagnostic_snapshot().table_panels[0].clone();
                        assert_eq!(restored.offsets, before.offsets);
                        assert_eq!(restored.focused, before.focused);
                        assert_eq!(restored.selection, before.selection);
                        assert!(restored.selected_for_wheel);
                        assert!(
                            native
                                .focused(app)
                                .is_some_and(|focus| focus == keyboard_focus)
                        );
                        assert_eq!(
                            panel
                                .read(app)
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            outer_before
                        );
                    }
                    let endpoint = cell_point(panel.read(app), 2, 1);
                    click(native, app, endpoint, true);
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].selection,
                        Some(((1, 0), (2, 1)))
                    );
                    key(native, app, "shift-right", false);
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].selection,
                        Some(((1, 0), (2, 2)))
                    );
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
                    key(native, app, "shift-end", false);
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().table_panels[0].selection,
                        Some(((1, 0), (3, 1))),
                        "an unrealized extension must preserve its last coherent cell rectangle"
                    );
                    assert!(
                        panel.read(app).resource_demand().is_some(),
                        "unrealized navigation must request owner-projected cells"
                    );
                })
                .unwrap();
                let outer = panel
                    .read_with(cx, |panel, _| {
                        panel
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px
                    })
                    .unwrap()
                    .max(outer);
                with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).table_viewport_for_test().unwrap().1;
                    wheel(native, app, bounds.center(), -400.0, -1600.0);
                    key(native, app, "escape", false);
                    let diagnostics = panel.read(app).diagnostic_snapshot();
                    assert!(diagnostics.table_panels[0].offsets.0 > 0.0);
                    assert!(diagnostics.table_panels[0].offsets.1 > 0.0);
                    assert!(diagnostics.table_panels[0].selected_for_wheel);
                    assert_eq!(
                        panel
                            .read(app)
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px,
                        outer
                    );
                })
                .unwrap();
                ready(window, &panel, cx).await;
                panel
                    .read_with(cx, |panel, _| {
                        let diagnostics = panel.diagnostic_snapshot();
                        let table = &diagnostics.table_panels[0];
                        eprintln!("native Table demanded diagnostics: {table:?}");
                        assert_eq!(table.id, identity);
                        assert!(table.realized_rows.start > 0);
                        assert!(table.realized_columns.start > 0);
                        assert!(table.cells <= 128);
                        assert!((table.realized_rows.end - table.realized_rows.start) <= 15);
                        assert!((table.realized_columns.end - table.realized_columns.start) <= 8);
                        assert!(
                            table.selection.is_none(),
                            "released cells must not pin an obsolete selection"
                        );
                        assert!(panel.table_source_pages_for_test().len() <= 128);
                        assert_eq!(
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            outer
                        );
                    })
                    .unwrap();
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
