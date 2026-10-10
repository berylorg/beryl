#![cfg(feature = "test-faults")]

#[path = "mounted_exact_status_controls/compaction.rs"]
mod compaction;
#[path = "pending_composer_activation/support.rs"]
mod composer_support;
#[path = "mounted_exact_status_controls/context.rs"]
mod context;
#[path = "normal_terminal/server.rs"]
mod server;
#[path = "mounted_exact_status_controls/stop_feedback.rs"]
mod stop_feedback;
#[path = "mounted_exact_status_controls/support.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;

mod cas_projection {
    pub use beryl_app::cas_projection::*;
}
mod composer_host {
    pub use beryl_app::composer_host::*;
}
mod composer_marker_seal {
    pub use beryl_app::composer_marker_seal::*;
}
mod submission_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/submission_fixture.rs"
    ));
}

use beryl_app::cas_projection::*;
use beryl_app::main_window::*;
use beryl_app::window_acquisition::*;
use beryl_backend::{
    DynamicToolCallResponse, ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions,
};
use beryl_home_store::{CommandCancellation, CommandOutcome, HomeCommand};
use beryl_model::*;
use gpui::AppContext;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use syndic_storage::SyndicTimestamp;

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

struct NoopLifecycle;
impl beryl_app::LifecycleYieldRequestHandler for NoopLifecycle {
    fn respond_lifecycle_yield(
        &mut self,
        _: OrdinaryDynamicToolContext,
        _: beryl_app::LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused lifecycle handler")
    }
}
struct NoopBranch;
impl beryl_app::BranchDiscussionResolutionRequestHandler for NoopBranch {
    fn respond_branch_discussion_resolution(
        &mut self,
        _: beryl_app::cas_projection::BranchDiscussionResolutionContext,
        _: beryl_app::BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused branch handler")
    }
}

#[gpui::test]
fn mounted_pointer_stop_closes_command_menu_preserves_waiting_and_exact_interrupted_feedback(
    cx: &mut gpui::TestAppContext,
) {
    run_mounted(cx, true, false, None);
}

#[gpui::test]
fn mounted_context_tracks_ordered_usage_and_expired_publication_without_interrupting_execution(
    cx: &mut gpui::TestAppContext,
) {
    run_mounted(cx, false, true, None);
}

#[gpui::test]
fn mounted_durable_selection_loss_rejects_the_old_popup_without_interrupting_execution(
    cx: &mut gpui::TestAppContext,
) {
    run_mounted(cx, false, false, None);
}

#[gpui::test]
fn mounted_durable_nondispatch_requires_a_fresh_exact_observation(cx: &mut gpui::TestAppContext) {
    run_mounted(
        cx,
        false,
        false,
        Some((ExactStopFeedbackState::DurableNondispatch, 1)),
    );
}

#[gpui::test]
fn mounted_volatile_nondispatch_remains_disabled_after_fresh_observations(
    cx: &mut gpui::TestAppContext,
) {
    run_mounted(
        cx,
        false,
        false,
        Some((ExactStopFeedbackState::VolatileNondispatch, 1)),
    );
}

#[gpui::test]
fn mounted_retained_feedback_capacity_refuses_new_commands_without_eviction(
    cx: &mut gpui::TestAppContext,
) {
    run_mounted(
        cx,
        false,
        false,
        Some((ExactStopFeedbackState::RequestNotAdmitted, 72)),
    );
}

fn run_mounted(
    cx: &mut gpui::TestAppContext,
    stop: bool,
    retire: bool,
    projected: Option<(ExactStopFeedbackState, usize)>,
) {
    run_mounted_with_notices(cx, stop, retire, projected, None);
}

fn run_mounted_with_notices(
    cx: &mut gpui::TestAppContext,
    stop: bool,
    retire: bool,
    projected: Option<(ExactStopFeedbackState, usize)>,
    notice_scenario: Option<stop_feedback::Scenario>,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let server = if stop {
        server::NormalTerminalServer::spawn_stop_feedback_terminal("interrupted")
    } else {
        server::NormalTerminalServer::spawn_controlled_connection_loss()
    };
    let endpoint = server.endpoint();
    let (fixture, prepared, acquisition) = support::join(
        support::worker(move || {
            let mut fixture = syndic::Fixture::new(189);
            eprintln!("mounted status fixture: {}", fixture.home_path().display());
            let (prepared, acquisition) = support::prepare_shell(&mut fixture);
            (fixture, prepared, acquisition)
        }),
        cx,
    );
    let worker = fixture.store.exact_stop_worker();
    let mut publication = Some(Arc::new(()));
    let appearance = cx.update(|app| {
        beryl_app::theme_runtime::GpuiAppearanceWindowSet::new(
            prepared.appearance().clone(),
            std::num::NonZeroUsize::new(4).unwrap(),
            app,
        )
    });
    let shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, appearance)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("mounted shell"))
    });
    let window = shell.window();
    window
        .update(cx, |root, window, cx| {
            root.test_mount_exact_status_worker(
                worker.clone(),
                Arc::downgrade(publication.as_ref().unwrap()),
                fixture.state.session(),
                window,
                cx,
            );
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        support::draw(window, cx);
        if window
            .read_with(cx, |root, app| {
                root.test_exact_status_selection_present(app)
            })
            .unwrap()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "composer selection did not become presentable"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_exact_status_diagnostics().0)
            .unwrap(),
        "Unknown"
    );
    let (fixture, session, projection, coordinator) = support::join(
        support::worker(move || {
            let mut fixture = fixture;
            fixture.submit_text(server::SUBMITTED_TEXT);
            let connector =
                ManagedBackendClientConnector::for_lifecycle_test(endpoint, server::AUTHORIZATION);
            let mut session = fixture
                .store
                .admit_runtime_lifecycle_test_candidate(
                    &connector,
                    syndic::execution_binding(),
                    CasProcessGeneration::new(48_189).unwrap(),
                    std::path::Path::new(EXECUTION_ROOT),
                    server::TIMEOUT,
                )
                .unwrap();
            let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
            let request = CasProjectionRequest::new(
                fixture.thread,
                fixture.selected_path(fixture.thread),
                syndic::execution_binding(),
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                SyndicTimestamp::from_unix_millis(48_100),
                server::TIMEOUT,
            );
            let projection = coordinator
                .obtain_projection(
                    &*fixture.home(),
                    &fixture.storage,
                    &mut session,
                    &request,
                    &fixture.cancellation,
                )
                .unwrap();
            (fixture, session, projection, coordinator)
        }),
        cx,
    );
    server.wait_for_projection();
    let old_snapshot = std::thread::scope(|scope| {
        let capture = std::thread::Builder::new()
            .name("mounted-status-execution".into())
            .stack_size(32 * 1024 * 1024)
            .spawn_scoped(scope, || {
                coordinator.execute_ordinary_turn(
                    &*fixture.home(),
                    &fixture.storage,
                    &fixture.state.assets(),
                    None,
                    projection,
                    &fixture.cancellation,
                    &OrdinaryTurnExecutionRequest::new(
                        TurnStartOptions::default(),
                        server::TIMEOUT,
                    ),
                    OrdinaryDynamicToolHandlers::new(&mut NoopLifecycle, &mut NoopBranch),
                )
            })
            .unwrap();
        let direct = support::join(
            support::worker({
                let worker = worker.clone();
                let thread = fixture.thread;
                move || {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    loop {
                        let snapshot = worker.selected_operation_snapshot(thread);
                        if snapshot.operation_active || Instant::now() >= deadline {
                            break snapshot;
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            }),
            cx,
        );
        eprintln!(
            "direct status: {:?}, active={}, selected={}",
            direct.state,
            direct.operation_active,
            window
                .read_with(cx, |root, app| root
                    .test_exact_status_selection_present(app))
                .unwrap()
        );
        support::wait(window, cx, |diagnostic| {
            diagnostic.0 == "working" && diagnostic.2
        });
        if retire {
            context::exercise(&server, window, cx, worker.clone(), fixture.thread);
        }
        if let Some(scenario) = notice_scenario {
            stop_feedback::exercise(
                scenario,
                window,
                cx,
                &worker,
                fixture.thread,
                &mut publication,
                fixture.state.session(),
            );
            window
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            server.close_connection();
            let _ = capture.join().unwrap();
            return direct;
        }
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        let anchor = visual.debug_bounds("main-window-status-turn").unwrap();
        let prior_focus = window
            .update(&mut visual, |root, window, cx| {
                let composer = root
                    .controller()
                    .unwrap()
                    .composer_mount()
                    .unwrap()
                    .read(cx)
                    .contribution()
                    .unwrap();
                composer
                    .read(cx)
                    .gpui_input()
                    .update(cx, |input, _| input.focus(window));
                window.focused(cx).unwrap()
            })
            .unwrap();
        visual.simulate_click(anchor.center(), gpui::Modifiers::none());
        assert!(
            window
                .read_with(&visual, |root, _| root.test_exact_status_diagnostics().1)
                .unwrap()
        );
        visual.simulate_keystrokes("escape");
        assert!(
            !window
                .read_with(&visual, |root, _| root.test_exact_status_diagnostics().1)
                .unwrap()
        );
        assert!(
            window
                .update(&mut visual, |_, window, _| prior_focus.is_focused(window))
                .unwrap()
        );
        visual.simulate_click(anchor.center(), gpui::Modifiers::none());
        if stop {
            window
                .update(&mut visual, |root, _, cx| {
                    root.test_set_shutdown_interaction_gated(true, cx)
                })
                .unwrap()
                .unwrap();
            visual.simulate_keystrokes("enter");
            assert_eq!(
                window
                    .read_with(&visual, |root, _| root.test_exact_status_diagnostics())
                    .unwrap(),
                ("working", true, false, 0, None)
            );
            window
                .update(&mut visual, |root, _, cx| {
                    root.test_set_shutdown_interaction_gated(false, cx)
                })
                .unwrap()
                .unwrap();
            let menu = visual.debug_bounds("main-window-turn-menu").unwrap();
            let row = visual.debug_bounds("main-window-soft-stop").unwrap();
            assert!(menu.origin.x >= gpui::px(0.) && menu.origin.y >= gpui::px(0.));
            visual.simulate_click(row.center(), gpui::Modifiers::none());
            visual.simulate_click(row.center(), gpui::Modifiers::none());
            assert!(
                !window
                    .read_with(&visual, |root, _| root.test_exact_status_diagnostics().1)
                    .unwrap()
            );
            drop(visual);
            support::wait(window, cx, |diagnostic| {
                diagnostic.4 == Some(ExactStopFeedbackState::Waiting)
            });
            assert!(
                window
                    .read_with(cx, |root, _| root.notice_projection().is_none())
                    .unwrap()
            );
            let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
            visual.simulate_click(anchor.center(), gpui::Modifiers::none());
            assert!(
                !window
                    .read_with(&visual, |root, _| root.test_exact_status_diagnostics().2)
                    .unwrap()
            );
            visual.simulate_keystrokes("enter space");
            assert_eq!(
                visual.debug_bounds("main-window-status-turn").unwrap(),
                anchor
            );
            drop(visual);
            server.release_stop_terminal();
            support::wait(window, cx, |diagnostic| {
                diagnostic.0 == "interrupted" && !diagnostic.1
            });
            window
                .read_with(cx, |root, _| {
                    let notice = root.notice_projection().unwrap();
                    assert_eq!(notice.kind, NoticeKind::ExactStopFeedback);
                    assert_eq!(notice.content.variant, NoticeVariant::Info);
                    assert_eq!(notice.content.dismissal, NoticeDismissal::Dismissible);
                    assert_eq!(notice.content.commands().count(), 0);
                })
                .unwrap();
            assert_eq!(
                window
                    .read_with(cx, |root, _| root.test_exact_status_diagnostics().3)
                    .unwrap(),
                1
            );
        } else {
            if let Some((state, count)) = projected {
                drop(visual);
                if state == ExactStopFeedbackState::DurableNondispatch {
                    let old_stamp = window
                        .read_with(cx, |root, app| {
                            root.test_exact_status_observation_stamp(app)
                        })
                        .unwrap();
                    publication = Some(Arc::new(()));
                    window
                        .update(cx, |root, window, cx| {
                            root.test_mount_exact_status_worker(
                                worker.clone(),
                                Arc::downgrade(publication.as_ref().unwrap()),
                                fixture.state.session(),
                                window,
                                cx,
                            )
                        })
                        .unwrap();
                    support::wait(window, cx, |d| d.0 == "working" && d.2);
                    let mut input = gpui::VisualTestContext::from_window(window.into(), cx);
                    input.simulate_click(anchor.center(), gpui::Modifiers::none());
                    window
                        .update(&mut input, |root, window, cx| {
                            let before = root.test_exact_status_diagnostics();
                            root.test_apply_exact_status_observation(
                                old_stamp,
                                ExactSelectedOperationSnapshot::unavailable(),
                                window,
                                cx,
                            );
                            assert_eq!(root.test_exact_status_diagnostics(), before);
                            assert!(before.1 && before.2);
                        })
                        .unwrap();
                    drop(input);
                }
                let feedback = support::join(
                    support::worker({
                        let worker = worker.clone();
                        let thread = fixture.thread;
                        move || {
                            (0..count)
                                .map(|_| {
                                    let ExactSoftStopAvailability::Eligible(eligibility) =
                                        worker.exact_soft_stop_eligibility(thread)
                                    else {
                                        panic!("projected feedback needs an exact source token");
                                    };
                                    eligibility.test_projected_feedback(state).unwrap()
                                })
                                .collect::<Vec<_>>()
                        }
                    }),
                    cx,
                );
                window
                    .update(cx, |root, _, cx| {
                        for feedback in feedback {
                            root.test_retain_exact_status_feedback(feedback, cx);
                        }
                        assert!(!root.test_exact_status_diagnostics().2);
                    })
                    .unwrap();
                let enabled = state == ExactStopFeedbackState::DurableNondispatch;
                support::wait(window, cx, |diagnostic| {
                    diagnostic.0 == "working"
                        && diagnostic.2 == enabled
                        && diagnostic.3 == count
                        && diagnostic.4 == Some(state)
                });
                visual = gpui::VisualTestContext::from_window(window.into(), cx);
                if !enabled {
                    visual.simulate_keystrokes("enter space");
                    assert_eq!(
                        window
                            .read_with(&visual, |root, _| root.test_exact_status_diagnostics().3)
                            .unwrap(),
                        count
                    );
                }
            }
            if projected.is_none() && !retire {
                std::thread::Builder::new()
                    .name("mounted-selection-release".into())
                    .stack_size(32 * 1024 * 1024)
                    .spawn_scoped(scope, || {
                        let home = fixture.home();
                        let session = fixture.state.session();
                        let bootstrap = session.minimal_bootstrap(&home).unwrap().unwrap();
                        let record = bootstrap
                            .windows()
                            .iter()
                            .find(|record| record.window_id() == WindowId::from_bytes([190; 16]))
                            .unwrap();
                        let mut command = HomeCommand::new(home.home_revision().unwrap());
                        command
                            .add(session.remove_window(
                                session.revision(&home).unwrap(),
                                beryl_state::RemoveSessionWindow::new(
                                    bootstrap.header().revision(),
                                    record.window_id(),
                                    record.revision(),
                                    record.selected_thread(),
                                ),
                            ))
                            .unwrap();
                        assert!(matches!(
                            home.execute(command),
                            CommandOutcome::Committed {
                                later_failure: None,
                                ..
                            }
                        ));
                    })
                    .unwrap()
                    .join()
                    .unwrap();
            }
            if retire {
                drop(publication.take());
            }
            if projected.is_none() {
                visual.simulate_keystrokes("enter");
            }
            drop(visual);
            support::draw(window, cx);
            if retire {
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_context_remaining_percent())
                        .unwrap(),
                    None
                );
            }
            assert_eq!(
                window
                    .read_with(cx, |root, _| root.test_exact_status_diagnostics().3)
                    .unwrap(),
                projected.map_or(0, |(_, count)| count)
            );
            window
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            server.close_connection();
        }
        let _ = capture.join().unwrap();
        direct
    });
    session.invalidate_connection();
    drop(session);
    server.join();
    let fixture = if stop {
        let old_stamp = window
            .read_with(cx, |root, app| {
                root.test_exact_status_observation_stamp(app)
            })
            .unwrap();
        let (fixture, successor) = support::join(
            support::worker(move || {
                let mut fixture = fixture;
                fixture.advance_clock_to(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64
                        + 1_000,
                );
                fixture.set_submission_identity_counter(300);
                fixture.submit_text(" successor operation");
                let successor = fixture
                    .store
                    .exact_stop_worker()
                    .selected_operation_snapshot(fixture.thread);
                assert_ne!(old_snapshot.origin, successor.origin);
                (fixture, successor)
            }),
            cx,
        );
        window
            .update(cx, |root, window, cx| {
                root.test_apply_exact_status_observation(old_stamp, successor, window, cx)
            })
            .unwrap();
        let successor_stamp = window
            .read_with(cx, |root, app| {
                root.test_exact_status_observation_stamp(app)
            })
            .unwrap();
        assert_ne!(successor_stamp.1, old_stamp.1);
        window
            .update(cx, |root, window, cx| {
                root.test_apply_exact_status_observation(
                    old_stamp,
                    ExactSelectedOperationSnapshot::unavailable(),
                    window,
                    cx,
                );
                root.test_apply_retained_stop_completion(successor_stamp, window, cx);
                assert_eq!(
                    root.test_exact_status_observation_stamp(cx),
                    successor_stamp
                );
                assert_eq!(
                    root.test_exact_status_diagnostics(),
                    ("Unknown", false, false, 1, None)
                );
            })
            .unwrap();
        fixture
    } else {
        fixture
    };
    if stop {
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    }
    drop(shell);
    drop(acquisition);
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}
