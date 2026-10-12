use super::*;
use crate::running_owner::InterruptedExitRecoveryOutcome;
use crate::syndic_transcript::SyndicTranscriptPanel;
use gpui::{AsyncApp, Entity, WindowHandle};

#[test]
fn native_failed_home_recovery_loads_indexed_code_and_routes_fresh_inner_demand() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some(&format!(
                    "```rust\n{}\n```",
                    "let payload = 1;\n".repeat(2000)
                )),
            )
        },
        |owner, faults, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let prior = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_range(window, &prior, 0, cx).await;
                let prior_identity = prior
                    .read_with(cx, |panel, _| panel.code_presentation_for_test().unwrap().0)
                    .unwrap();
                let selected = snapshot(&owner).windows()[0].selected_thread();
                let (home, generation) = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    (
                        graph.home().service_reference(),
                        graph.home().health().generation().unwrap(),
                    )
                };
                owner.borrow_mut().test_enable_resident_frame_capture();
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                assert!(home.home_revision().is_err());
                cx.update(|app| {
                    RunningProcessOwner::test_observe_running_home_failure(&owner, app)
                })
                .unwrap();
                let mut recovered = false;
                for step in 0..750 {
                    draw(window, cx);
                    cx.update(|app| {
                        RunningProcessOwner::test_dispatch_scheduled_resident_frame(&owner, app);
                        RunningProcessOwner::test_observe_running_home_failure(&owner, app);
                    })
                    .unwrap();
                    if step == 150 || step == 500 {
                        let retained = owner.borrow();
                        eprintln!(
                            "Code recovery progress: captured={}, outcome={}, stage={}",
                            retained.test_running_home_recovery_identity().is_some(),
                            retained.automatic_recovery_outcome().is_some(),
                            if retained.test_services_on_worker() {
                                "worker".into()
                            } else {
                                retained.test_services().test_recovery_retirement_stage()
                            }
                        );
                    }
                    recovered = match owner.borrow().automatic_recovery_outcome().as_deref() {
                        Some(InterruptedExitRecoveryOutcome::Completed) => true,
                        Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                            panic!("Code recovery unavailable: {error}")
                        }
                        Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                            panic!("Code recovery cancelled")
                        }
                        _ => false,
                    };
                    if recovered {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(recovered, "failed-home recovery must complete");
                let current = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                assert_eq!(current.entity_id(), prior.entity_id());
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    assert_ne!(graph.home().health().generation().unwrap(), generation);
                }
                assert_eq!(snapshot(&owner).windows()[0].selected_thread(), selected);
                assert_eq!(windows(&owner), vec![window]);
                drop(home);
                wait_range(window, &current, 0, cx).await;
                current
                    .read_with(cx, |panel, _| {
                        assert_eq!(
                            panel.code_presentation_for_test().unwrap().0,
                            prior_identity
                        );
                        assert!(!panel.code_viewport_for_test().unwrap().3);
                        assert!(panel.painted_glyphs_for_test() > 50);
                        assert!(panel.diagnostic_snapshot().code_panels[0].logical_lines >= 2000);
                    })
                    .unwrap();
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        let bounds = current.read(app).code_viewport_for_test().unwrap().1;
                        code_interaction::press(native, app, bounds.center());
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position: bounds.center(),
                                delta: gpui::ScrollDelta::Pixels(gpui::point(
                                    gpui::px(0.0),
                                    gpui::px(-90.5),
                                )),
                                modifiers: Default::default(),
                                touch_phase: gpui::TouchPhase::Moved,
                            }),
                            app,
                        );
                        native.draw_and_present_for_test(app);
                    })
                    .unwrap();
                })
                .unwrap();
                wait_range(window, &current, 4, cx).await;
                code_anchor::scroll_beyond_resident_lines(
                    window,
                    &current,
                    "let payload = 1;\n".len() as u64,
                    cx,
                )
                .await;
                drop(prior);
                window
                    .update(cx, |root, native, app| {
                        root.set_ordinary_close_interaction_gated(true, app);
                        root.suspend_running_thread_reads(native, app);
                    })
                    .unwrap();
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
                assert!(drained);
                window
                    .update(cx, |root, native, app| {
                        root.retire_running_thread_views(native, app).unwrap()
                    })
                    .unwrap();
                current.update(cx, |panel, _| panel.retire()).unwrap();
                draw(window, cx);
                assert!(
                    current
                        .read_with(cx, |panel, _| panel
                            .diagnostic_snapshot()
                            .code_panels
                            .is_empty())
                        .unwrap()
                );
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}

fn draw(window: WindowHandle<MainWindowShellRoot>, cx: &mut AsyncApp) {
    cx.update(|app| {
        app.update_window(window.into(), |_, native, app| {
            native.draw_and_present_for_test(app);
            native.minimize_window();
        })
        .unwrap();
    })
    .unwrap();
}

async fn wait_range(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    top: u64,
    cx: &mut AsyncApp,
) {
    for _ in 0..300 {
        draw(window, cx);
        if panel.read_with(cx, |panel, _| {
            let Some((resource, _, actual_top, _)) = panel.code_viewport_for_test() else { return false; };
            actual_top == top && panel.indexed_resource_response(&resource).is_some_and(|response| {
                matches!(&response.value, IndexedResourceValue::Range(range)
                    if range.request_complete && !range.items.is_empty() && range.items.len() <= 16
                    && matches!(range.cursor.request, index::Request::Code { lines } if lines.start == top.saturating_sub(2)))
            })
        }).unwrap() {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    panic!("recovered Code must receive its provider-backed range at line {top}");
}
