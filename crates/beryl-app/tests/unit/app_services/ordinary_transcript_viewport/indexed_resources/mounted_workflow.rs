use super::code_long_line::{wait_until, wait_until_unminimized, with_native};
use super::narrative_native::{
    copy_both, drag, key, point_at, pointer, ready, record, require_fixture_ledger, writer,
};
use super::*;
use crate::syndic_transcript::{
    ResidentPresentationRecordKind, ResidentRecordSource, SyndicTranscriptPanel,
};
use beryl_model::SyndicThreadId;
use gpui::{Entity, EntityInputHandler, Focusable, PlatformInput, point, px};

#[path = "mounted_workflow/support.rs"]
mod workflow;
use workflow::*;

#[path = "mounted_workflow/composer.rs"]
mod composer_interaction;
use composer_interaction::*;

#[test]
fn native_mounted_mixed_transcript_interactions_navigation_invalidation_and_disposal() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained_and_workers_and_deadline(
        1,
        0,
        9,
        Duration::from_secs(120),
        |path| {
            let suffix = format!(
                " after\n\n```rust\n{}```\n\n| A | B |\n| --- | --- |\n| one | two |\n\n{}",
                "let sample = 1;\n".repeat(2000),
                super::table_native_mount::large_table()
            );
            super::image_markers::fixture::seed_content(path, "before **café 🌸 words** ", &suffix);
            seed_other_thread(path);
        },
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
                                panel.measured_frame_for_test().is_some()
                                    && panel
                                        .table_kind_geometry_for_test(true)
                                        .is_some_and(|geometry| geometry.1)
                            })
                            .unwrap()
                    },
                    "mixed saved history must publish measured Tail with the indexed Table",
                )
                .await;
                let original = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let frame = panel.read(app).measured_frame_for_test().unwrap();
                    let viewport = panel.read(app).viewport_bounds_for_test().unwrap();
                    let tail = frame.records.last().unwrap();
                    assert!(
                        (tail.top_px + tail.height_px - f32::from(viewport.size.height)).abs()
                            < 0.1
                    );
                    let outside = point(viewport.left() + px(1.0), viewport.center().y);
                    let before = frame.manual_scroll_total_px;
                    wheel(native, app, outside, 0.0, 12.5);
                    assert_eq!(
                        panel
                            .read(app)
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px,
                        before - 12.5
                    );
                    wheel(native, app, outside, 0.0, -12.5);
                })
                .unwrap();
                place(window, &panel, cx, |panel| {
                    panel
                        .table_kind_geometry_for_test(true)
                        .map(|geometry| geometry.6.center())
                })
                .await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel
                                    .table_kind_geometry_for_test(true)
                                    .is_some_and(|geometry| geometry.1)
                            })
                            .unwrap()
                    },
                    "indexed Table source and geometry must settle",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let (id, current, body, _, row_height, column_width, commands) =
                        panel.read(app).table_kind_geometry_for_test(true).unwrap();
                    assert!(current);
                    let p = body.origin + point(px(column_width / 2.0), px(row_height / 2.0));
                    click(native, app, p);
                    let before = panel
                        .read(app)
                        .diagnostic_snapshot()
                        .table_panels
                        .iter()
                        .find(|table| table.id == id)
                        .unwrap()
                        .clone();
                    assert_eq!((before.rows, before.columns), (24, 12));
                    assert_eq!(before.selection, Some(((1, 0), (1, 0))));
                    let outer = panel
                        .read(app)
                        .measured_frame_for_test()
                        .unwrap()
                        .manual_scroll_total_px;
                    let header = commands.center();
                    wheel(native, app, header, -10.25, -12.5);
                    let after = panel
                        .read(app)
                        .diagnostic_snapshot()
                        .table_panels
                        .iter()
                        .find(|table| table.id == before.id)
                        .unwrap()
                        .clone();
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
                    wheel(native, app, header, 10.25, 12.5);
                })
                .unwrap();
                place(window, &panel, cx, |panel| {
                    panel
                        .table_kind_geometry_for_test(false)
                        .map(|geometry| geometry.6.center())
                })
                .await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel
                                    .table_kind_geometry_for_test(false)
                                    .is_some_and(|geometry| geometry.1)
                            })
                            .unwrap()
                    },
                    "inline Table must paint its exact prepared source owner",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let (id, _, body, _, row_height, column_width, _) =
                        panel.read(app).table_kind_geometry_for_test(false).unwrap();
                    click(
                        native,
                        app,
                        body.origin + point(px(column_width / 2.0), px(row_height / 2.0)),
                    );
                    let diagnostics = panel.read(app).diagnostic_snapshot();
                    let inline = diagnostics
                        .table_panels
                        .iter()
                        .find(|table| table.id == id)
                        .unwrap();
                    assert_eq!((inline.rows, inline.columns), (1, 2));
                    assert_eq!(inline.selection, Some(((1, 0), (1, 0))));
                })
                .unwrap();
                place(window, &panel, cx, |panel| {
                    panel
                        .code_command_geometry_for_test()
                        .map(|geometry| geometry.0.center())
                })
                .await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.code_commands_current_for_test()
                                    && panel.code_painted_body_glyphs_for_test() > 0
                            })
                            .unwrap()
                    },
                    "Code must mount after the outer Table navigation",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let p = panel.read(app).code_viewport_for_test().unwrap().1.center();
                    click(native, app, p);
                    let outer = panel
                        .read(app)
                        .measured_frame_for_test()
                        .unwrap()
                        .manual_scroll_total_px;
                    let before = panel.read(app).code_presentation_for_test().unwrap().2;
                    wheel(native, app, p, 0.0, -10.5);
                    assert_eq!(
                        panel.read(app).code_presentation_for_test().unwrap().2,
                        before + 10.5
                    );
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
                leading(window, &panel, cx).await;
                ready(window, &panel, cx, &["before", "after"]).await;
                settled_transcript(window, &panel, cx).await;
                let (copied, _) = writer(&panel, cx);
                select_words(window, &panel, cx);
                with_native(window, cx, |native, app| {
                    copy_both(
                        native,
                        app,
                        &copied,
                        super::branch_discussion::SELECTED,
                        "café 🌸 words",
                    )
                })
                .unwrap();
                let resident = composer(window, cx).await;
                with_native(window, cx, |native, app| {
                    key(native, app, "shift-f10");
                    key(native, app, "enter");
                })
                .unwrap();
                editor_ready(
                    window,
                    &resident,
                    super::branch_discussion::SELECTED.len() as u64,
                    cx,
                )
                .await;
                assert_eq!(
                    copied.borrow().len(),
                    2,
                    "Quote must preserve the paired clipboard values"
                );
                with_native(window, cx, |native, app| {
                    assert!(panel.read(app).focus_handle(app).is_focused(native));
                })
                .unwrap();
                assert_eq!(
                    copy_all(window, &resident, cx).await,
                    super::branch_discussion::SELECTED
                );
                clear_editor(window, &resident, cx).await;
                leading(window, &panel, cx).await;
                inspect_marker(window, &panel, cx).await;
                edit_available(window, &resident, cx).await;
                settled_transcript(window, &panel, cx).await;
                with_native(window, cx, |native, _| native.activate_window()).unwrap();
                wait_until_unminimized(
                    window,
                    cx,
                    |cx| {
                        with_native(window, cx, |native, app| {
                            native.is_window_active()
                                && panel.read(app).narrative_geometry_current_for_test(native)
                        })
                        .unwrap()
                    },
                    "native window restoration must settle before the actual resize",
                )
                .await;
                select_words(window, &panel, cx);
                edit_available(window, &resident, cx).await;
                let width_before = panel
                    .read_with(cx, |panel, _| {
                        panel.viewport_bounds_for_test().unwrap().size.width
                    })
                    .unwrap();
                let native_width_before =
                    with_native(window, cx, |native, _| native.viewport_size().width).unwrap();
                with_native(window, cx, |native, app| {
                    key(native, app, "shift-f10");
                    assert!(
                        panel
                            .read(app)
                            .transcript_menu_rows_for_test()
                            .iter()
                            .any(|row| row.0 == "Edit message" && row.1),
                        "current user menu must enable Edit after the exact empty composer qualification: {:?}", panel.read(app).transcript_menu_rows_for_test()
                    );
                    let size = native.viewport_size();
                    native.resize(gpui::size(size.width + px(100.0), size.height));
                })
                .unwrap();
                let mut width_observations = 0_u32;
                wait_until_unminimized(
                    window,
                    cx,
                    |cx| {
                        with_native(window, cx, |native, app| {
                            let panel = panel.read(app);
                            if width_observations % 50 == 0 {
                                eprintln!(
                                    "mounted resize publication: native_before={native_width_before:?}, panel_before={width_before:?}, native={:?}, window={:?}, panel={:?}, current={}, pending_measurement={}",
                                    native.viewport_size(),
                                    native.window_bounds(),
                                    panel.viewport_bounds_for_test(),
                                    panel.narrative_geometry_current_for_test(native),
                                    panel.measurement_diagnostics().0
                                );
                            }
                            width_observations += 1;
                            native.viewport_size().width > native_width_before
                                && panel.viewport_bounds_for_test()
                                    .is_some_and(|bounds| bounds.size.width > width_before)
                                && panel.narrative_geometry_current_for_test(native)
                                && !panel.measurement_diagnostics().0
                        })
                        .unwrap()
                    },
                    "requested native resize must publish actual wider current geometry",
                )
                .await;
                ready(window, &panel, cx, &["before"]).await;
                with_native(window, cx, |native, app| {
                    assert!(!panel.read(app).quote_menu_open_for_test());
                    assert!(
                        panel
                            .read(app)
                            .narrative_selection_ranges_for_test()
                            .is_none()
                    );
                    let before = copied.borrow().len();
                    key(native, app, "ctrl-c");
                    assert_eq!(copied.borrow().len(), before);
                })
                .unwrap();
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .viewport_bounds_for_test()
                            .unwrap()
                            .size
                            .width)
                        .unwrap()
                        > width_before
                );
                select_words(window, &panel, cx);
                with_native(window, cx, |native, app| {
                    copy_both(
                        native,
                        app,
                        &copied,
                        super::branch_discussion::SELECTED,
                        "café 🌸 words",
                    )
                })
                .unwrap();
                let words_width_before = words_width(&panel, cx);
                let appearance = window
                    .read_with(cx, |root, _| root.test_appearance())
                    .unwrap();
                let prepared = {
                    let owner = owner.borrow();
                    native_appearance::system_font_appearance_with_narrative_size(
                        owner.test_services().graph().unwrap().state(),
                        28,
                    )
                };
                let enlarged =
                    std::sync::Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                        appearance.number().checked_next().unwrap(),
                        prepared,
                        crate::theme_runtime::AppearancePublication::Durable,
                    ));
                with_native(window, cx, |native, app| key(native, app, "shift-f10")).unwrap();
                cx.update(|app| MainWindowShellRoot::test_apply_appearance(window, enlarged, app))
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let before = copied.borrow().len();
                    key(native, app, "ctrl-c");
                    assert_eq!(copied.borrow().len(), before);
                    assert!(!panel.read(app).quote_menu_open_for_test());
                })
                .unwrap();
                ready(window, &panel, cx, &["before"]).await;
                assert!(
                    words_width(&panel, cx) > words_width_before,
                    "the mounted glyph geometry must grow under the larger narrative font"
                );
                select_words(window, &panel, cx);
                with_native(window, cx, |native, app| {
                    copy_both(
                        native,
                        app,
                        &copied,
                        super::branch_discussion::SELECTED,
                        "café 🌸 words",
                    )
                })
                .unwrap();
                let old_record = panel
                    .read_with(cx, |panel, _| record(panel, "before").0)
                    .unwrap();
                with_native(window, cx, |native, app| key(native, app, "shift-f10")).unwrap();
                activate(window, OTHER_THREAD, cx).await;
                ready(window, &panel, cx, &["branch prefix"]).await;
                with_native(window, cx, |native, app| {
                    assert!(!panel.read(app).quote_menu_open_for_test());
                    assert!(panel.read(app).narrative_selection_ranges_for_test().is_none());
                    let snapshot = panel.read(app).snapshot();
                    assert!(snapshot.records.iter().all(|record| record.id != old_record));
                    assert!(snapshot.records.iter().any(|record| matches!(&record.provenance.source, ResidentRecordSource::Syndic(source) if source.view_id.0 == OTHER_THREAD.to_string())));
                    panel.read(app).focus_handle(app).focus(native);
                    let before = copied.borrow().len();
                    key(native, app, "enter");
                    key(native, app, "ctrl-c");
                    assert_eq!(copied.borrow().len(), before);
                })
                .unwrap();
                activate(window, original, cx).await;
                leading(window, &panel, cx).await;
                ready(window, &panel, cx, &["before"]).await;
                for _ in 0..2 {
                    activate(window, OTHER_THREAD, cx).await;
                    ready(window, &panel, cx, &["branch prefix"]).await;
                    bounded(window, &panel, cx);
                    panel
                        .read_with(cx, |panel, _| {
                            assert!(panel.diagnostic_snapshot().code_panels.is_empty());
                            assert!(panel.diagnostic_snapshot().table_panels.is_empty());
                            assert!(!panel.quote_menu_open_for_test());
                            assert!(panel.narrative_selection_ranges_for_test().is_none());
                        })
                        .unwrap();
                    activate(window, original, cx).await;
                    leading(window, &panel, cx).await;
                    ready(window, &panel, cx, &["before"]).await;
                    bounded(window, &panel, cx);
                    assert!(
                        panel
                            .read_with(cx, |panel, _| panel
                                .narrative_selection_ranges_for_test()
                                .is_none())
                            .unwrap()
                    );
                    select_words(window, &panel, cx);
                    with_native(window, cx, |native, app| {
                        copy_both(
                            native,
                            app,
                            &copied,
                            super::branch_discussion::SELECTED,
                            "café 🌸 words",
                        )
                    })
                    .unwrap();
                }
                dispose(window, &panel, cx).await;
            })
        },
    );
}

#[path = "mounted_workflow/replacement_branch.rs"]
mod replacement_branch;

#[path = "mounted_workflow/image_inspection.rs"]
mod image_inspection;

#[path = "mounted_workflow/original_commands.rs"]
mod original_commands;
