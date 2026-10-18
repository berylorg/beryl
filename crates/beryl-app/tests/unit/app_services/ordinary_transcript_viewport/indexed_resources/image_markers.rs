use super::code_long_line::{retire_code_fixture, wait_until, with_native};
use super::narrative_native::{
    drag, key, point_at, pointer, ready, record, require_fixture_ledger,
};
use super::*;
use crate::syndic_transcript::{
    ImageMarkerInspectionRequested, ResidentContextMenuContentKind, ResidentPresentationRecordKind,
};
use beryl_home_store::HomeStore;
use beryl_state::AssetState;
use std::{cell::RefCell, num::NonZeroU64, rc::Rc};

#[path = "image_markers/fixture.rs"]
pub(super) mod fixture;

#[test]
fn native_grouped_markers_keep_inline_bounds_exact_origins_and_primary_keyboard_commands() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(1, 0, fixture::seed, |owner, _, cx| {
        Box::pin(async move {
            let window = windows(&owner)[0];
            let root = cx.update(|app| window.entity(app).unwrap()).unwrap();
            let panel = window
                .read_with(cx, |root, _| root.test_transcript_panel())
                .unwrap();
            ready(window, &panel, cx, &["before", "after"]).await;
            wait_until(
                window,
                cx,
                |cx| {
                    panel
                        .read_with(cx, |panel, _| {
                            panel.code_viewport_for_test().is_some()
                                && panel.inline_table_viewport_for_test().is_some()
                                && panel.code_command_geometry_for_test().is_some()
                        })
                        .unwrap()
                },
                "Code and Table origins must have mounted geometry",
            )
            .await;
            let target_outer_records = panel
                .read_with(cx, |panel, _| {
                    assert!(panel.dimmed_record_ids_for_test().is_empty());
                    panel
                        .snapshot()
                        .records
                        .iter()
                        .filter(|record| {
                            matches!(
                                record.kind,
                                ResidentPresentationRecordKind::AuthoredInlineParts { .. }
                                    | ResidentPresentationRecordKind::InlineTable { .. }
                                    | ResidentPresentationRecordKind::ResourceReference {
                                        resource_kind: crate::syndic_transcript::ResourceKind::Code,
                                        ..
                                    }
                            )
                        })
                        .map(|record| record.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap();
            assert_eq!(
                target_outer_records.len(),
                3,
                "fixture must expose the authored group, Code, and Table outer records"
            );
            let mut resource_source = None;
            for code in [true, false] {
                let label = if code { "Code" } else { "Table" };
                with_native(window, cx, |native, app| {
                    native.draw_and_present_for_test(app);
                    let viewport = panel.read(app).viewport_bounds_for_test().unwrap();
                    let position = resource_point(panel.read(app), code)
                        .expect("prepared resource content origin");
                    pointer(
                        native,
                        app,
                        gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: gpui::point(
                                viewport.left() + gpui::px(1.0),
                                viewport.center().y,
                            ),
                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                gpui::px(0.0),
                                viewport.center().y - position.y,
                            )),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                    );
                })
                .unwrap();
                wait_until(window, cx, |cx| with_native(window, cx, |native, app| {
                    let panel = panel.read(app);
                    panel.narrative_geometry_current_for_test(native)
                        && if code { panel.code_commands_current_for_test() } else { panel.table_commands_current_for_test() }
                        && resource_point(panel, code).zip(panel.viewport_bounds_for_test())
                            .is_some_and(|(point, viewport)| viewport.contains(&point))
                }).unwrap(), "exact resource content origin must be visible and current after native scrolling").await;
                with_native(window, cx, |native, app| {
                    let position = resource_point(panel.read(app), code).unwrap();
                    let snapshot = panel.read(app).snapshot();
                    let expected = snapshot.records.iter().find(|record| if code {
                        matches!(record.kind, ResidentPresentationRecordKind::ResourceReference { resource_kind: crate::syndic_transcript::ResourceKind::Code, .. })
                    } else { matches!(record.kind, ResidentPresentationRecordKind::InlineTable { .. }) }).unwrap().id.clone();
                    context(native, app, position);
                    let target = panel.read(app).resident_context_menu_target().unwrap_or_else(|| panic!("{label} native context at {position:?} must retain exact target {expected:?}; geometry current={}, rows={:?}, callback={:?}", panel.read(app).narrative_geometry_current_for_test(native), panel.read(app).transcript_menu_rows_for_test(), panel.read(app).transcript_menu_open_diagnostic_for_test()));
                    assert_eq!(target.record.record_id, expected, "{label} must target its own outer record");
                    let exact = target.record.historical_target().unwrap();
                    if let Some(previous) = &resource_source {
                        assert_eq!(&exact.item_id, previous);
                    } else { resource_source = Some(exact.item_id); }
                    let rows = panel.read(app).transcript_menu_rows_for_test();
                    assert_eq!(rows.last().unwrap().0, "Edit message");
                    assert!(rows.last().unwrap().1 || rows.last().unwrap().2.is_some());
                    key(native, app, "escape");
                }).unwrap();
            }
            let resource_source = resource_source.unwrap();
            with_native(window, cx, |native, app| {
                let viewport = panel.read(app).viewport_bounds_for_test().unwrap();
                pointer(
                    native,
                    app,
                    gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                        position: gpui::point(viewport.left() + gpui::px(1.0), viewport.center().y),
                        delta: gpui::ScrollDelta::Pixels(gpui::point(
                            gpui::px(0.0),
                            gpui::px(100_000.0),
                        )),
                        modifiers: Default::default(),
                        touch_phase: gpui::TouchPhase::Moved,
                    }),
                );
            })
            .unwrap();
            wait_until(
                window,
                cx,
                |cx| {
                    with_native(window, cx, |native, app| {
                        let panel = panel.read(app);
                        if !panel.narrative_geometry_current_for_test(native) {
                            return false;
                        }
                        let Some(viewport) = panel.viewport_bounds_for_test() else {
                            return false;
                        };
                        ["before", "after"].into_iter().all(|needle| {
                            let (id, _, _) = record(panel, needle);
                            panel
                                .narrative_position_for_test(&id, 2)
                                .is_some_and(|point| viewport.contains(&point))
                        })
                    })
                    .unwrap()
                },
                "grouped selection glyphs must be visible after native outer scrolling",
            )
            .await;
            let observed = Rc::new(RefCell::new(Vec::<ImageMarkerInspectionRequested>::new()));
            let capture = observed.clone();
            let _subscription = cx
                .update(|app| {
                    app.subscribe(
                        &panel,
                        move |_, event: &ImageMarkerInspectionRequested, _| {
                            capture.borrow_mut().push(event.clone());
                        },
                    )
                })
                .unwrap();
            let markers = with_native(window, cx, |native, app| {
                native.draw_and_present_for_test(app);
                let snapshot = panel.read(app).snapshot();
                let group = snapshot.records.iter().find(|record| matches!(record.kind, ResidentPresentationRecordKind::AuthoredInlineParts { .. })).unwrap();
                let markers = group.authored_parts().iter().filter(|record| matches!(record.kind, ResidentPresentationRecordKind::ImageMarker(_)))
                    .map(|record| record.id.clone()).collect::<Vec<_>>();
                assert_eq!(markers.len(), 2);
                let first = panel.read(app).image_marker_bounds(&markers[0], native).unwrap();
                let second = panel.read(app).image_marker_bounds(&markers[1], native).unwrap();
                assert_eq!(first.origin.y, second.origin.y);
                assert!(first.right() <= second.left());
                let (before_id, _, before_display) = record(panel.read(app), "before");
                let before = point_at(panel.read(app), &before_id, before_display.len());
                assert!(before.x <= first.left());
                assert!((f32::from(before.y - first.top())).abs() < 20.0);
                let (after_id, _, _) = record(panel.read(app), "after");
                let after = point_at(panel.read(app), &after_id, 0);
                assert!(after.x >= second.right());
                assert!((f32::from(after.y - second.top())).abs() < 20.0);
                assert!(panel.read(app).measurement_diagnostics().2 <= 32);
                let selection_start = point_at(panel.read(app), &before_id, 2);
                let selection_end = point_at(panel.read(app), &after_id, 2);
                assert_eq!(panel.read(app).narrative_hit_for_test(selection_start), Some((before_id.clone(), 2)), "the first authored part's glyph position must hit its own exact identity: {}", panel.read(app).narrative_hit_diagnostic_for_test(selection_start));
                assert_eq!(panel.read(app).narrative_hit_for_test(selection_end), Some((after_id.clone(), 2)), "the final authored part's glyph position must hit its own exact identity: {}", panel.read(app).narrative_hit_diagnostic_for_test(selection_end));
                drag(native, app, selection_start, selection_end);
                let selection = panel.read(app).narrative_selection_ranges_for_test().unwrap();
                for marker in &markers {
                    assert_eq!(selection.iter().find(|(id, _)| id == marker).map(|(_, range)| range.clone()), Some(0..1), "drag {selection_start:?} -> {selection_end:?} must include exact marker {marker:?}; actual owner ranges: {selection:?}");
                }
                click(native, app, point_at(panel.read(app), &before_id, 2));
                let mut source = None;
                for (id, bounds) in markers.iter().zip([first, second]) {
                    context(native, app, bounds.center());
                    let target = panel.read(app).resident_context_menu_target().unwrap();
                    assert_eq!(&target.record.record_id, id);
                    assert!(matches!(target.record.content_kind, ResidentContextMenuContentKind::ImageMarker(_)));
                    assert!(target.record.historical_target().is_some());
                    source.get_or_insert(target.record.historical_target().unwrap());
                    let rows = panel.read(app).transcript_menu_rows_for_test();
                    assert_eq!(rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(), ["Copy", "Save", "Edit message"]);
                    assert!(rows[..2].iter().all(|row| !row.1 && row.2.is_some()));
                    assert!(rows[2].1 || rows[2].2.is_some());
                    key(native, app, "enter");
                    assert!(panel.read(app).quote_menu_open_for_test());
                    key(native, app, "escape");
                }
                let source = source.unwrap();
                assert_eq!(source.item_id, resource_source);
                for position in [point_at(panel.read(app), &before_id, 2)] {
                    context(native, app, position);
                    let target = panel.read(app).resident_context_menu_target().unwrap();
                    let exact = target.record.historical_target().unwrap();
                    assert_eq!(exact.item_id, source.item_id);
                    assert_eq!(exact.turn_id, source.turn_id);
                    assert_eq!(exact.thread_id, source.thread_id);
                    let rows = panel.read(app).transcript_menu_rows_for_test();
                    assert_eq!(rows.last().unwrap().0, "Edit message");
                    assert!(rows.last().unwrap().1 || rows.last().unwrap().2.is_some());
                    key(native, app, "escape");
                }
                markers
            }).unwrap();
            for (index, command) in [None, Some("enter"), Some("space")].into_iter().enumerate() {
                let (bounds, diagnostic) = with_native(window, cx, |native, app| {
                    native.draw_and_present_for_test(app);
                    let bounds = panel
                        .read(app)
                        .image_marker_bounds(&markers[0], native)
                        .unwrap();
                    assert!(
                        panel
                            .read(app)
                            .viewport_bounds_for_test()
                            .unwrap()
                            .contains(&bounds.center())
                    );
                    if let Some(command) = command {
                        key(native, app, command);
                    } else {
                        click(native, app, bounds.center());
                    }
                    (
                        bounds,
                        panel
                            .read(app)
                            .image_marker_inspection_diagnostic_for_test(&markers[0], native),
                    )
                })
                .unwrap();
                assert_eq!(
                    observed.borrow().len(),
                    index + 1,
                    "marker primary command {command:?} must deliver after its app update; {diagnostic}"
                );
                let event = observed.borrow()[index].clone();
                with_native(window, cx, |native, app| {
                    native.draw_and_present_for_test(app);
                    assert_eq!(event.target.record.record_id, markers[0]);
                    assert_eq!(event.bounds, bounds);
                    assert!(
                        panel
                            .read(app)
                            .image_marker_inspection_current(&event, native)
                    );
                    let visible = root.read(app).test_image_marker_inspection().unwrap();
                    assert_eq!(visible.request, event.request);
                    assert_eq!(visible.target, event.target);
                    key(native, app, "escape");
                })
                .unwrap();
                with_native(window, cx, |native, app| {
                    assert!(root.read(app).test_image_marker_inspection().is_none());
                    assert!(event.focus.is_focused(native));
                })
                .unwrap();
            }
            with_native(window, cx, |native, app| {
                key(native, app, "shift-f10");
                assert!(panel.read(app).quote_menu_open_for_test());
                assert_eq!(
                    panel
                        .read(app)
                        .resident_context_menu_target()
                        .unwrap()
                        .record
                        .record_id,
                    markers[0]
                );
                key(native, app, "escape");
                assert!(panel.read(app).dimmed_record_ids_for_test().is_empty());
                let first = panel
                    .read(app)
                    .image_marker_bounds(&markers[0], native)
                    .unwrap();
                context(native, app, first.center());
                let rows = panel.read(app).transcript_menu_rows_for_test();
                assert_eq!(rows.last().unwrap().0, "Edit message");
                assert!(
                    rows.last().unwrap().1,
                    "exact historical menu should admit the idle empty composer"
                );
                assert_eq!(
                    panel.read(app).transcript_menu_focused_id_for_test(),
                    Some("transcript-image-copy")
                );
                key(native, app, "end");
                assert_eq!(
                    panel.read(app).transcript_menu_focused_id_for_test(),
                    Some("transcript-edit-message")
                );
                key(native, app, "enter");
                assert!(!panel.read(app).quote_menu_open_for_test());
                native.minimize_window();
            })
            .unwrap();
            let resident = composer(window, cx).await;
            wait_until(
                window,
                cx,
                |cx| {
                    resident
                        .read_with(cx, |resident, app| {
                            assert_eq!(resident.last_error(), None);
                            let state = resident.test_historical_replacement_state();
                            state.1.is_some()
                                && !state.0
                                && resident.gpui_input().read(app).is_quiescent()
                                && resident
                                    .gpui_input()
                                    .read(app)
                                    .is_surface_current_and_interactive()
                                && target_outer_records.iter().all(|id| {
                                    panel.read(app).dimmed_record_ids_for_test().contains(id)
                                })
                        })
                        .unwrap()
                },
                "exact image-origin Edit did not publish the imported composer",
            )
            .await;
            with_native(window, cx, |native, app| {
                native.draw_and_present_for_test(app);
                assert!(
                    target_outer_records
                        .iter()
                        .all(|id| panel.read(app).dimmed_record_ids_for_test().contains(id)),
                    "published replacement must dim all three historical outer records"
                );
            })
            .unwrap();
            let input = resident
                .read_with(cx, |resident, _| resident.gpui_input())
                .unwrap();
            let before = input
                .read_with(cx, |input, _| {
                    assert_eq!(input.surface().unwrap().realized_objects().len(), 2);
                    input
                        .export_restoration(Some(input.history_frontier()))
                        .unwrap()
                })
                .unwrap();
            with_native(window, cx, |native, app| {
                input.update(app, |input, _| input.focus(native));
                key(native, app, "escape");
            })
            .unwrap();
            wait_until(
                window,
                cx,
                |cx| {
                    resident
                        .read_with(cx, |resident, app| {
                            let state = resident.test_historical_replacement_state();
                            !state.0
                                && state.1.is_none()
                                && resident.gpui_input().read(app).is_quiescent()
                                && panel.read(app).dimmed_record_ids_for_test().is_empty()
                        })
                        .unwrap()
                },
                "image-origin replacement cancellation did not settle",
            )
            .await;
            with_native(window, cx, |native, app| {
                native.draw_and_present_for_test(app);
                assert!(
                    panel.read(app).dimmed_record_ids_for_test().is_empty(),
                    "Escape must clear historical dimming in the rendered panel"
                );
            })
            .unwrap();
            assert_eq!(
                input
                    .read_with(cx, |input, _| input
                        .export_restoration(Some(input.history_frontier()))
                        .unwrap())
                    .unwrap(),
                before
            );
            retire_code_fixture(window, &panel, cx).await;
        })
    });
}

fn resource_point(
    panel: &crate::syndic_transcript::SyndicTranscriptPanel,
    code: bool,
) -> Option<gpui::Point<gpui::Pixels>> {
    if code {
        let viewport = panel.code_viewport_for_test()?.1;
        let header_command = panel.code_command_geometry_for_test()?.0;
        Some(gpui::point(
            viewport.left() + gpui::px(20.0),
            header_command.center().y,
        ))
    } else {
        let header = panel.inline_table_viewport_for_test()?.1;
        Some(gpui::point(
            header.left() + gpui::px(20.0),
            header.center().y,
        ))
    }
}

fn context(native: &mut gpui::Window, app: &mut gpui::App, position: gpui::Point<gpui::Pixels>) {
    pointer(
        native,
        app,
        gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Right,
            click_count: 1,
            ..Default::default()
        }),
    );
}

fn click(native: &mut gpui::Window, app: &mut gpui::App, position: gpui::Point<gpui::Pixels>) {
    pointer(
        native,
        app,
        gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
    );
    pointer(
        native,
        app,
        gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
    );
}
