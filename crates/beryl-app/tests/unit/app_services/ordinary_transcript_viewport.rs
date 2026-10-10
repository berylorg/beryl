use super::*;

fn seed_history(path: &std::path::Path) {
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
    for ordinal in 0..32u8 {
        let item = SyndicItemId::from_bytes([ordinal + 40; 16]);
        let text = format!("Transcript viewport record {ordinal} ").repeat(if ordinal % 3 == 0 {
            70
        } else {
            1
        });
        let timestamp =
            |offset| SyndicTimestamp::from_unix_millis(1000 + u64::from(ordinal) * 10 + offset);
        let turn = exact_cas::submit_current_draft(
            &home,
            syndic.clone(),
            thread,
            SyndicDraftId::from_bytes([ordinal + 80; 16]),
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
                    .update(cx, |root, _, app| {
                        let entity = root.test_transcript_panel();
                        let panel = entity.read(app);
                        let frame = panel.measured_frame_for_test()?;
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
