use super::code_long_line::{
    retire_code_fixture, wait_until_unminimized, wait_until_with_visibility, with_native,
};
use super::*;
use crate::syndic_transcript::ResidentPresentationRecordKind;
use gpui::{Focusable, px};

#[path = "narrative_native/interaction.rs"]
mod interaction;

pub(super) use interaction::{
    copy_both, drag, key, point_at, pointer, ready, record, require_fixture_ledger, writer,
};
use interaction::{press, ready_with_visibility, release};

pub(super) fn keyboard_copy_focus_contract() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some("# Title\r\n\r\nprefix **café 🌸 words** suffix and \\* literal\r\n"),
            )
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["prefix"]).await;
                let (captured, refuse) = writer(&panel, cx);
                with_native(window, cx, |native, app| {
                    let (id, source, display) = record(panel.read(app), "prefix");
                    assert!(source.contains("**café 🌸 words**"));
                    assert!(!display.contains("**"));
                    let start = display.find("café 🌸 words").unwrap();
                    let end = start + "café 🌸 words".len();
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, start),
                        point_at(panel.read(app), &id, end),
                    );
                    copy_both(native, app, &captured, "**café 🌸 words**", "café 🌸 words");
                    let partial = display.find("fé 🌸").unwrap();
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, partial + "fé 🌸".len()),
                        point_at(panel.read(app), &id, partial),
                    );
                    copy_both(native, app, &captured, "fé 🌸", "fé 🌸");
                    let escaped = display.find("* literal").unwrap();
                    let point = point_at(panel.read(app), &id, escaped);
                    press(native, app, point, false);
                    release(native, app, point);
                    let point = point_at(panel.read(app), &id, escaped + 1);
                    press(native, app, point, true);
                    release(native, app, point);
                    copy_both(native, app, &captured, "\\*", "*");
                    native.minimize_window();
                })
                .unwrap();
                let other = cx.update(|app| app.focus_handle()).unwrap();
                with_native(window, cx, |native, app| {
                    native.focus(&other);
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(20))
                    .await;
                with_native(window, cx, |native, app| {
                    native.focus(&panel.read(app).focus_handle(app));
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(20))
                    .await;
                with_native(window, cx, |native, app| key(native, app, "ctrl-c")).unwrap();
                assert_eq!(captured.borrow().last().unwrap(), "\\*");
                let before = captured.borrow().len();
                refuse.set(true);
                with_native(window, cx, |native, app| key(native, app, "ctrl-c")).unwrap();
                assert_eq!(captured.borrow().len(), before);
                retire_code_fixture(window, &panel, cx).await;
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .narrative_layout_facts_for_test()
                            .is_empty())
                        .unwrap()
                );
            })
        },
    );
}

#[test]
fn native_narrative_long_ordered_marker_fits_measured_record() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("123456789. x\n")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["123456789. x"]).await;
                let (captured, _) = writer(&panel, cx);
                with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "123456789. x");
                    let (body, marker, content) = panel
                        .read(app)
                        .narrative_marker_geometry_for_test(&id)
                        .unwrap();
                    assert!(marker > body, "long marker must exercise wrapping");
                    assert!(content >= marker);
                    let row = panel
                        .read(app)
                        .measured_frame_for_test()
                        .unwrap()
                        .records
                        .into_iter()
                        .find(|row| row.record_id == id)
                        .unwrap();
                    assert!(row.height_px >= content);
                    assert_eq!(display.trim_end_matches('\n'), "x");
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, 0),
                        point_at(panel.read(app), &id, 1),
                    );
                    copy_both(native, app, &captured, "123456789. x", "x");
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_narrative_wrapped_unicode_selection_crosses_coherent_spans_and_remeasures() {
    require_fixture_ledger();
    let paragraph = format!("wrapped {}tail", "café 🌸 words ".repeat(24));
    let source = format!("first **alpha**\r\n\r\n{paragraph}\r\n");
    let saved_source = source.clone();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        move |path| seed_history_text(path, 1, Some(&saved_source)),
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                super::code_long_line::native_geometry::prepare_width_for_shrink(
                    window,
                    cx,
                    px(180.0),
                )
                .await;
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready_with_visibility(window, &panel, cx, &["first **alpha**", "wrapped"], false)
                    .await;
                let (captured, _) = writer(&panel, cx);
                let wrapped_id = with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "wrapped");
                    let end = display.find("tail").unwrap() + 4;
                    let start_point = point_at(panel.read(app), &id, 0);
                    let end_point = point_at(panel.read(app), &id, end);
                    assert!(
                        end_point.y > start_point.y,
                        "fixture must select genuinely wrapped native text"
                    );
                    drag(native, app, start_point, end_point);
                    copy_both(native, app, &captured, &paragraph, &paragraph);
                    let (first, _, first_display) = record(panel.read(app), "first **alpha**");
                    let start = first_display.find("alpha").unwrap();
                    let until = display.find('🌸').unwrap() + '🌸'.len_utf8();
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &first, start),
                        point_at(panel.read(app), &id, until),
                    );
                    let expected_start = source.find("**alpha**").unwrap();
                    let expected_end = source.find("🌸").unwrap() + '🌸'.len_utf8();
                    copy_both(
                        native,
                        app,
                        &captured,
                        &source[expected_start..expected_end],
                        "alpha\n\nwrapped café 🌸",
                    );
                    id
                })
                .unwrap();
                let before = captured.borrow().len();
                let old_width = with_native(window, cx, |native, _| {
                    let width = native.viewport_size().width;
                    native.resize(gpui::size(width - px(180.0), native.viewport_size().height));
                    width
                })
                .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let changed = with_native(window, cx, |native, app| {
                        if native.viewport_size().width == old_width {
                            return false;
                        }
                        assert!(native.viewport_size().width < old_width);
                        key(native, app, "ctrl-c");
                        native.minimize_window();
                        true
                    })
                    .unwrap();
                    if changed {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "native resize must change the actual Window viewport before obsolete Copy is tested"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert_eq!(
                    captured.borrow().len(),
                    before,
                    "obsolete width geometry cannot authorize Copy"
                );
                ready(window, &panel, cx, &["wrapped"]).await;
                with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "wrapped");
                    assert_eq!(id, wrapped_id);
                    let start = display.find("café 🌸").unwrap();
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, start),
                        point_at(panel.read(app), &id, start + "café 🌸".len()),
                    );
                    copy_both(native, app, &captured, "café 🌸", "café 🌸");
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .narrative_layout_facts_for_test()
                            .is_empty())
                        .unwrap()
                );
            })
        },
    );
}

#[test]
fn native_narrative_pointer_selection_cannot_cross_nested_code_or_table() {
    for nested in [
        "```text\nnested boundary\n```",
        "| nested | cell |\n| --- | --- |\n| boundary | value |",
    ] {
        require_fixture_ledger();
        let source = format!("before **alpha**\n\n{nested}\n\nafter **omega**\n");
        run_mounted_with_prepared_home_retained(
            1,
            0,
            move |path| seed_history_text(path, 1, Some(&source)),
            move |owner, _, cx| {
                Box::pin(async move {
                    let window = windows(&owner)[0];
                    let panel = window
                        .read_with(cx, |root, _| root.test_transcript_panel())
                        .unwrap();
                    let old_height = with_native(window, cx, |native, _| {
                        let height = native.viewport_size().height;
                        native.resize(gpui::size(native.viewport_size().width, height + px(240.0)));
                        height
                    })
                    .unwrap();
                    wait_until_unminimized(window, cx, |cx| with_native(window, cx, |native, _|
                        native.viewport_size().height > old_height).unwrap(),
                        "nested boundary fixture must admit its larger physical native viewport before minimizing").await;
                    wait_until_with_visibility(window, cx, |cx| {
                        with_native(window, cx, |native, app| {
                            let panel = panel.read(app);
                            panel.narrative_geometry_current_for_test(native)
                                && if nested.starts_with("```") {
                                    panel.code_commands_current_for_test()
                                        && panel.diagnostic_snapshot().code_panels.iter().any(|code| code.layout_state == "ready" && code.layout_fragments > 0)
                                } else {
                                    panel.table_commands_current_for_test()
                                        && panel.diagnostic_snapshot().table_panels.iter().any(|table| table.cells > 0 && !table.pending)
                                }
                        }).unwrap()
                    }, "nested resource must finish its candidate and publish current geometry before narrative interaction", true).await;
                    ready(window, &panel, cx, &["before **alpha**", "after **omega**"]).await;
                    wait_until_with_visibility(window, cx, |cx| with_native(window, cx, |native, app| {
                        let panel = panel.read(app);
                        if !panel.narrative_geometry_current_for_test(native) { return false; }
                        let Some(bounds) = panel.viewport_bounds_for_test() else { return false; };
                        let (before, _, first) = record(panel, "before **alpha**");
                        let (after, _, last) = record(panel, "after **omega**");
                        let start = first.find("alpha").unwrap();
                        let end = last.find("omega").unwrap() + 5;
                        let Some(first_point) = panel.narrative_position_for_test(&before, start) else { return false; };
                        let Some(selected_end) = panel.narrative_position_for_test(&before, start + 5) else { return false; };
                        let Some(cross_end) = panel.narrative_position_for_test(&after, end) else { return false; };
                        bounds.contains(&first_point) && bounds.contains(&selected_end) && bounds.contains(&cross_end)
                    }).unwrap(), "nested boundary selection endpoints must be inside the visible native viewport", true).await;
                    let (captured, _) = writer(&panel, cx);
                    with_native(window, cx, |native, app| {
                        assert!(panel.read(app).narrative_geometry_current_for_test(native),
                            "published nested-resource frame must remain current at pointer dispatch");
                        let (before, _, first) = record(panel.read(app), "before **alpha**");
                        let (after, _, last) = record(panel.read(app), "after **omega**");
                        let start = first.find("alpha").unwrap();
                        let bounds = panel.read(app).viewport_bounds_for_test().unwrap();
                        let first_point = point_at(panel.read(app), &before, start);
                        let selected_end = point_at(panel.read(app), &before, start + 5);
                        assert!(bounds.contains(&first_point) && bounds.contains(&selected_end), "initial drag must reach visible painted glyphs");
                        drag(
                            native,
                            app,
                            first_point,
                            selected_end,
                        );
                        key(native, app, "ctrl-c");
                        assert_eq!(captured.borrow().as_slice(), &["**alpha**".to_owned()]);
                        let end = last.find("omega").unwrap() + 5;
                        let first_point = point_at(panel.read(app), &before, start);
                        let cross_end = point_at(panel.read(app), &after, end);
                        assert!(bounds.contains(&first_point) && bounds.contains(&cross_end), "cross-boundary drag must reach visible painted glyphs");
                        drag(
                            native,
                            app,
                            first_point,
                            cross_end,
                        );
                        key(native, app, "ctrl-c");
                        assert!(
                            captured.borrow().iter().all(|text| text == "**alpha**"),
                            "nested resources cannot become ordinary narrative intervals"
                        );
                    })
                    .unwrap();
                    retire_code_fixture(window, &panel, cx).await;
                })
            },
        );
    }
}

#[test]
fn native_narrative_capacity_refusal_retains_source_and_excludes_unavailable_label_selection() {
    require_fixture_ledger();
    let source = "*x*.".repeat(700);
    let saved = source.clone();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        move |path| seed_history_text(path, 1, Some(&saved)),
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_until_with_visibility(window, cx, |cx| with_native(window, cx, |native, app| {
                let panel = panel.read(app);
                let snapshot = panel.snapshot();
                let unavailable = snapshot.records.iter().find(|record| matches!(
                    &record.kind,
                    ResidentPresentationRecordKind::NarrativeUnavailable { source: payload, reason }
                        if *reason == syndic_storage::narrative::NarrativePreparationErrorKind::Capacity
                            && matches!(payload.as_ref(), syndic_storage::ProjectionPayload::InlineMarkdown { source: raw, .. } if raw.as_ref() == source)
                ));
                panel.narrative_geometry_current_for_test(native) && unavailable.is_some_and(|record| panel.measured_frame_for_test().is_some_and(|frame|
                    frame.records.iter().any(|row| row.record_id == record.id && row.height_px > 0.0)))
            }).unwrap(), "genuine narrative capacity refusal must retain admitted source in a measured local unavailable row", true).await;
                let (captured, _) = writer(&panel, cx);
                with_native(window, cx, |native, app| {
                assert!(panel.read(app).narrative_geometry_current_for_test(native),
                    "refused source label must be tested through a current published frame");
                let snapshot = panel.read(app).snapshot();
                let unavailable = snapshot.records.iter().find(|record| matches!(
                    &record.kind,
                    ResidentPresentationRecordKind::NarrativeUnavailable { source: payload, .. }
                        if matches!(payload.as_ref(), syndic_storage::ProjectionPayload::InlineMarkdown { source: raw, .. } if raw.as_ref() == source)
                )).unwrap();
                let ResidentPresentationRecordKind::NarrativeUnavailable { source: payload, reason } = &unavailable.kind else { unreachable!() };
                assert_eq!(*reason, syndic_storage::narrative::NarrativePreparationErrorKind::Capacity);
                let syndic_storage::ProjectionPayload::InlineMarkdown { block_id, block_kind, span_ordinal, source_range, source: raw } = payload.as_ref() else { panic!("refusal preserves its exact admitted InlineMarkdown binding"); };
                assert_eq!(*block_kind, syndic_storage::MarkdownBlockKind::Paragraph);
                assert_eq!(*span_ordinal, 1);
                assert_eq!(source_range.len(), source.len() as u64);
                assert_eq!(raw.as_ref(), source);
                let crate::syndic_transcript::ResidentRecordSource::Syndic(binding) = &unavailable.provenance.source else {
                    panic!("capacity refusal retains genuine Syndic provenance");
                };
                assert_eq!(binding.block_id, Some(block_id.as_bytes()));
                assert_eq!(binding.source_range, Some(source_range.start()..source_range.end()));
                assert!(binding.item_id.is_some() && binding.turn_id.is_some());
                assert!(panel.read(app).narrative_position_for_test(&unavailable.id, 0).is_none());
                assert!(panel.read(app).narrative_position_for_test(&unavailable.id, source.len()).is_none());
                assert!(panel.read(app).narrative_layout_facts_for_test().iter().all(|(id, _, _, _)| id != &unavailable.id));
                let frame = panel.read(app).measured_frame_for_test().unwrap();
                let row = frame.records.iter().find(|row| row.record_id == unavailable.id).unwrap();
                assert!(row.height_px > 0.0);
                let bounds = panel.read(app).viewport_bounds_for_test().unwrap();
                let first = gpui::point(bounds.left() + px(24.0), bounds.top() + px(row.top_px + row.height_px * 0.5));
                let last = first + gpui::point(px(28.0), px(0.0));
                assert!(bounds.contains(&first) && bounds.contains(&last));
                drag(native, app, first, last);
                key(native, app, "ctrl-c");
                key(native, app, "ctrl-c");
                assert!(captured.borrow().is_empty(), "unavailable labels cannot become authored Copy text");
            }).unwrap();
                retire_code_fixture(window, &panel, cx).await;
                panel
                    .read_with(cx, |panel, _| {
                        assert!(panel.snapshot().records.is_empty());
                        assert!(panel.narrative_layout_facts_for_test().is_empty());
                    })
                    .unwrap();
            })
        },
    );
}
