use super::*;

fn resident_positions(panel: &crate::syndic_transcript::SyndicTranscriptPanel) -> Vec<u64> {
    panel
        .snapshot()
        .records
        .iter()
        .filter_map(|record| match &record.provenance.source {
            crate::syndic_transcript::ResidentRecordSource::Syndic(source) => {
                source.position.map(|position| position.0)
            }
            _ => None,
        })
        .collect()
}

#[path = "ordinary_transcript_viewport/resources.rs"]
mod resources;

#[path = "../../support/native_window_geometry.rs"]
mod native_geometry;

#[test]
fn native_mounted_transcript_scrolls_across_adjacent_seeds_in_both_directions() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_count(path, 160),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                for _ in 0..100 {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap()
                    })
                    .unwrap();
                    let ready = window
                        .update(cx, |root, _, app| {
                            root.test_transcript_panel()
                                .read(app)
                                .measured_frame_for_test()
                                .is_some_and(|frame| !frame.records.is_empty())
                                && root.coherent_selected_thread_title(app).is_some()
                        })
                        .unwrap();
                    if ready {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                let panel = window
                    .update(cx, |root, _, _| root.test_transcript_panel())
                    .unwrap();
                let initial = cx
                    .update(|app| resident_positions(panel.read(app)))
                    .unwrap();
                assert!(initial[0] > 1);
                let mut prior = initial.clone();
                for backward in [true, true, false, false, false, false] {
                    let before_scroll = cx
                        .update(|app| {
                            panel
                                .read(app)
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px
                        })
                        .unwrap();
                    let expected_scroll = before_scroll
                        + if backward {
                            -1_000_000.25
                        } else {
                            1_000_000.25
                        };
                    let anchor = cx.update(|app| {
                        let position = panel.read(app).viewport_bounds_for_test().unwrap().center();
                        app.update_window(window.into(), |_, native, app| {
                            native.dispatch_input_for_test(
                                gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                    position,
                                    delta: gpui::ScrollDelta::Pixels(gpui::point(
                                        gpui::px(0.0),
                                        gpui::px(if backward {
                                            1_000_000.25
                                        } else {
                                            -1_000_000.25
                                        }),
                                    )),
                                    modifiers: Default::default(),
                                    touch_phase: gpui::TouchPhase::Moved,
                                }),
                                app,
                            );
                            for _ in 0..8 {
                                native.refresh();
                                native.draw_and_present_for_test(app);
                            }
                            let current = panel.read(app);
                            let frame = current.measured_frame_for_test().unwrap();
                            assert_eq!(frame.manual_scroll_total_px, expected_scroll,
                                "manual input must publish before capturing the pre-extension anchor");
                            assert!(current.narrative_geometry_current_for_test(native));
                            assert_eq!(frame.clamp.as_ref().map(|clamp| clamp.direction), Some(if backward {
                                crate::syndic_transcript::TranscriptPageDirection::Backward
                            } else {
                                crate::syndic_transcript::TranscriptPageDirection::Forward
                            }));
                            assert_eq!(resident_positions(current), prior,
                                "the anchor must be captured before adjacent history publication");
                            frame.anchor.unwrap()
                        })
                        .unwrap()
                    })
                    .unwrap();
                    let mut extended = None;
                    for _ in 0..150 {
                        cx.update(|app| {
                            app.update_window(window.into(), |_, native, app| {
                                native.draw_and_present_for_test(app)
                            })
                            .unwrap()
                        })
                        .unwrap();
                        let positions = cx
                            .update(|app| resident_positions(panel.read(app)))
                            .unwrap();
                        let settled = window
                            .update(cx, |_, native, app| {
                                let panel = panel.read(app);
                                panel.narrative_geometry_current_for_test(native)
                                    && panel.measured_frame_for_test().is_some_and(|frame| {
                                        frame.manual_scroll_total_px == expected_scroll
                                    })
                            })
                            .unwrap();
                        let changed = if backward {
                            positions[0] < prior[0]
                        } else {
                            positions.last() > prior.last()
                        };
                        let terminal = if backward {
                            prior[0] == 1
                        } else {
                            prior.last() == Some(&160)
                        };
                        if settled && (changed || terminal) {
                            if changed {
                                if backward {
                                    assert!(positions.last() < prior.last());
                                } else {
                                    assert!(positions[0] > prior[0]);
                                }
                                cx.update(|app| {
                                    let panel = panel.read(app);
                                    let current =
                                        panel.measured_frame_for_test().unwrap().anchor.unwrap();
                                    assert_eq!(current.record_id, anchor.record_id);
                                    assert_eq!(current.viewport_y_px, anchor.viewport_y_px);
                                    assert!(panel.measurement_diagnostics().2 <= 32);
                                })
                                .unwrap();
                            }
                            extended = Some(positions);
                            break;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                    }
                    prior = extended.expect("ordinary host must deliver the exact adjacent page");
                    assert!(prior.len() <= 512);
                    if backward {
                        assert!(prior[0] <= initial[0]);
                    }
                }
                assert_eq!(prior.last(), Some(&160));
                cx.update(|app| {
                    let position = panel.read(app).viewport_bounds_for_test().unwrap().center();
                    app.update_window(window.into(), |_, native, app| {
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position,
                                delta: gpui::ScrollDelta::Pixels(gpui::point(
                                    gpui::px(0.0),
                                    gpui::px(1_000_000.25),
                                )),
                                modifiers: Default::default(),
                                touch_phase: gpui::TouchPhase::Moved,
                            }),
                            app,
                        );
                        for _ in 0..8 {
                            native.refresh();
                            native.draw_and_present_for_test(app);
                        }
                    })
                    .unwrap();
                })
                .unwrap();
                let mut fenced = None;
                for _ in 0..100 {
                    fenced = window
                        .update(cx, |root, native, app| {
                            root.test_fence_pending_adjacent_read(native, app)
                                .then(|| panel.read(app).snapshot())
                        })
                        .unwrap();
                    if fenced.is_some() {
                        break;
                    }
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app);
                        })
                        .unwrap()
                    })
                    .unwrap();
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                let before_disposal =
                    fenced.expect("actual adjacent worker must be pending before disposal fence");
                let mut drained = false;
                for _ in 0..100 {
                    drained = window
                        .update(cx, |root, _, _| {
                            root.release_suspended_running_thread_sources()
                        })
                        .unwrap();
                    if drained {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(
                    drained,
                    "cancelled adjacent output and worker must settle before source retirement"
                );
                window
                    .update(cx, |root, native, app| {
                        assert!(root.running_thread_reads_drained());
                        root.retire_running_thread_views(native, app).unwrap();
                    })
                    .unwrap();
                cx.update(|app| {
                    assert!(Arc::ptr_eq(
                        &before_disposal.records,
                        &panel.read(app).snapshot().records
                    ));
                    panel.update(app, |panel, _| panel.retire());
                })
                .unwrap();
                for _ in 0..4 {
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                    assert!(
                        cx.update(|app| panel.read(app).snapshot().is_empty())
                            .unwrap()
                    );
                }
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}

fn seed_history(path: &std::path::Path) {
    seed_history_count(path, 32);
}

fn seed_history_count(path: &std::path::Path, count: u8) {
    seed_history_text(path, count, None);
}

fn seed_history_text(path: &std::path::Path, count: u8, resource_text: Option<&str>) {
    use crate::support::exact_cas;
    use beryl_model::{SyndicDraftId, SyndicItemId};
    use syndic_storage::{SourceEventPayload, SyndicTimestamp, TurnEndStatus};
    let StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    } = support::open(path)
    else {
        panic!("viewport fixture Home")
    };
    let home = candidate.publish().unwrap();
    let thread = state
        .session()
        .minimal_bootstrap(&home)
        .unwrap()
        .unwrap()
        .windows()[0]
        .selected_thread()
        .unwrap()
        .thread_id();
    for ordinal in 0..count {
        let mut item_identity = [0xA7; 16];
        item_identity[0] = ordinal;
        let item = SyndicItemId::from_bytes(item_identity);
        let mut draft_identity = [0xA8; 16];
        draft_identity[0] = ordinal;
        let text = resource_text.map(str::to_owned).unwrap_or_else(|| {
            format!("Transcript viewport record {ordinal} ").repeat(if ordinal % 3 == 0 {
                70
            } else {
                1
            })
        });
        let timestamp =
            |offset| SyndicTimestamp::from_unix_millis(1000 + u64::from(ordinal) * 10 + offset);
        let turn = exact_cas::submit_current_draft(
            &home,
            syndic.clone(),
            thread,
            SyndicDraftId::from_bytes(draft_identity),
            item,
            &text,
            timestamp(0),
        );
        let source = exact_cas::establish_turn(&home, syndic.clone(), thread, turn, timestamp(1));
        exact_cas::admit_event(
            &home,
            syndic.clone(),
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(2),
        );
        exact_cas::correlate_user_item(
            &home,
            syndic.clone(),
            thread,
            turn,
            item,
            &source,
            timestamp(3),
        );
        exact_cas::admit_event(
            &home,
            syndic.clone(),
            thread,
            turn,
            &source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
            timestamp(4),
        );
        exact_cas::converge_and_release_terminal_history(&home, syndic.clone(), thread, turn);
    }
    drop((state, syndic));
    home.close().unwrap();
}

#[test]
fn native_mounted_transcript_publishes_measured_tail_and_routes_fractional_input() {
    run_mounted_with_prepared_home_retained(1, 0, seed_history, |owner, _, cx| {
        Box::pin(async move {
            let window = windows(&owner)[0];
            native_geometry::prepare_width_for_shrink(window, cx, gpui::px(120.0)).await;
            let mut ready = None;
            for _ in 0..100 {
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        native.draw_and_present_for_test(app)
                    })
                    .unwrap();
                })
                .unwrap();
                ready = window
                    .update(cx, |root, native, app| {
                        let entity = root.test_transcript_panel();
                        let panel = entity.read(app);
                        let frame = panel.measured_frame_for_test()?;
                        if frame.records.is_empty() || root.coherent_selected_thread_title(app).is_none()
                            || !panel.narrative_geometry_current_for_test(native)
                            || frame.records.iter().any(|row| matches!(panel.snapshot().records.get(row.index).map(|record| &record.kind),
                                Some(crate::syndic_transcript::ResidentPresentationRecordKind::NarrativePending { .. })))
                        { return None; }
                        let bounds = panel.viewport_bounds_for_test()?;
                        assert!(panel.measurement_diagnostics().2 <= 32);
                        assert!(root.coherent_selected_thread_title(app).is_some());
                        Some((entity, frame, bounds))
                    })
                    .unwrap();
                if ready.is_some() {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }
            let (panel, frame, bounds) =
                ready.expect("ordinary shell must publish measured attachment");
            let last = frame.records.last().unwrap();
            assert_eq!(last.index, 31);
            assert!((last.top_px + last.height_px - f32::from(bounds.size.height)).abs() < 0.01);
            assert!(frame.records.iter().any(|record| record.height_px > 72.0));
            assert!(frame.records.len() < 32);
            cx.update(|app| {
                app.update_window(window.into(), |_, native, app| {
                    native.draw(app).clear();
                    native.draw_and_present_for_test(app);
                    native.dispatch_input_for_test(
                        gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                gpui::px(0.0),
                                gpui::px(63.25),
                            )),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
                panel.read_with(app, |panel, _| {
                    assert!(panel.painted_glyphs_for_test() > 0);
                    let frame = panel.measured_frame_for_test().unwrap();
                    assert_eq!(frame.manual_scroll_total_px, -63.25);
                    assert!(matches!(
                        panel.refresh_placement(),
                        crate::syndic_transcript::TranscriptActivationPlacement::Position(_)
                    ));
                });
                app.update_window(window.into(), |_, native, app| {
                    native.dispatch_input_for_test(
                        gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                            button: gpui::MouseButton::Left,
                            position: bounds.center(),
                            modifiers: Default::default(),
                            click_count: 1,
                            first_mouse: false,
                        }),
                        app,
                    );
                    native.dispatch_input_for_test(
                        gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("up").unwrap(),
                            is_held: false,
                        }),
                        app,
                    );
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
                panel.read_with(app, |panel, _| {
                    assert_eq!(
                        panel
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px,
                        -103.25
                    );
                });
            })
            .unwrap();
            let anchor = cx
                .update(|app| {
                    let anchor = panel
                        .read(app)
                        .measured_frame_for_test()
                        .unwrap()
                        .anchor
                        .unwrap();
                    app.update_window(window.into(), |_, native, _| {
                        let viewport = native.viewport_size();
                        native.resize(gpui::size(
                            viewport.width - gpui::px(120.0),
                            viewport.height,
                        ));
                    })
                    .unwrap();
                    anchor
                })
                .unwrap();
            let mut resized = false;
            for _ in 0..100 {
                resized = cx
                    .update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap();
                        let panel = panel.read(app);
                        assert!(panel.measurement_diagnostics().2 <= 32);
                        if panel.measurement_diagnostics().0
                            || panel.viewport_bounds_for_test().unwrap().size.width
                                >= bounds.size.width
                        {
                            return false;
                        }
                        let current = panel.measured_frame_for_test().unwrap().anchor.unwrap();
                        assert_eq!(current.record_id, anchor.record_id);
                        assert_eq!(current.viewport_y_px, anchor.viewport_y_px);
                        true
                    })
                    .unwrap();
                if resized {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }
            assert!(
                resized,
                "native resize must preserve the detached semantic anchor"
            );
            cx.update(|app| {
                let position = panel.read(app).viewport_bounds_for_test().unwrap().center();
                app.update_window(window.into(), |_, native, app| {
                    native.dispatch_input_for_test(
                        gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position,
                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                gpui::px(0.0),
                                gpui::px(1000000.0),
                            )),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    for _ in 0..8 {
                        native.draw_and_present_for_test(app);
                    }
                })
                .unwrap();
                let frame = panel.read(app).measured_frame_for_test().unwrap();
                assert_eq!(frame.records.first().unwrap().index, 0);
                assert_eq!(frame.records.first().unwrap().top_px, 0.0);
                app.quit();
            })
            .unwrap();
        })
    });
}
