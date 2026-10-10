#![cfg(all(feature = "test-faults", target_os = "windows"))]

#[path = "support/native_shell_appearance.rs"]
mod appearance;
#[path = "pending_composer_activation/support.rs"]
mod composer_support;
#[path = "support/desktop_placement_native.rs"]
mod native;
#[path = "normal_terminal/server.rs"]
mod server;
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
use gpui::{AppContext, Application, AsyncApp};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
        _: BranchDiscussionResolutionContext,
        _: beryl_app::BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused branch handler")
    }
}

async fn join<T: Send + 'static>(work: std::thread::JoinHandle<T>, cx: &AsyncApp) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !work.is_finished() {
        assert!(Instant::now() < deadline, "native context worker timeout");
        native::pump(cx).await;
    }
    work.join().unwrap()
}

fn click(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    bounds: gpui::Bounds<gpui::Pixels>,
    cx: &mut AsyncApp,
) {
    let position = gpui::point(
        bounds.origin.x + bounds.size.width / 2.,
        bounds.origin.y + bounds.size.height / 2.,
    );
    cx.update_window(window.into(), |_, native, app| {
        native.dispatch_input_for_test(
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            }),
            app,
        );
        native.dispatch_input_for_test(
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                button: gpui::MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            }),
            app,
        );
    })
    .unwrap();
}

fn wire(input: i64) -> String {
    let breakdown = format!(
        r#"{{"totalTokens":99,"inputTokens":{input},"cachedInputTokens":1,"cacheWriteInputTokens":2,"outputTokens":3,"reasoningOutputTokens":4}}"#
    );
    format!(
        r#"{{"method":"thread/tokenUsage/updated","params":{{"threadId":"{}","turnId":"{}","tokenUsage":{{"total":{breakdown},"last":{breakdown},"modelContextWindow":200}}}},"emittedAtMs":1770000000123}}"#,
        server::CAS_THREAD_ID,
        server::CAS_TURN_ID
    )
}

fn verify_manual_admission_fences(fixture: &syndic::Fixture) {
    let worker = fixture.store.exact_stop_worker();
    let session = fixture.state.session();
    let window = WindowId::from_bytes([190; 16]);
    let selection = session
        .minimal_bootstrap(&fixture.home())
        .unwrap()
        .unwrap()
        .windows()
        .iter()
        .find(|record| record.window_id() == window)
        .unwrap()
        .selected_thread()
        .unwrap();
    let publication = Arc::new(());
    let ManualCompactionAvailability::Eligible(eligibility) = worker
        .test_selected_compaction_availability(
            &session,
            window,
            selection,
            Arc::downgrade(&publication),
        )
    else {
        panic!("exact idle capability unavailable");
    };
    let before = fixture.home().home_revision().unwrap();
    let mut retained = (0..71)
        .map(|_| worker.test_reserve_compaction_feedback().unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        worker.request_manual_compaction(&eligibility),
        Err(ExactStopRequestError::Capacity)
    ));
    assert_eq!(fixture.home().home_revision().unwrap(), before);
    let duplicate = retained[0].clone();
    retained.remove(0);
    assert!(matches!(
        worker.test_reserve_compaction_feedback(),
        Err(ExactStopRequestError::Capacity)
    ));
    drop(duplicate);
    retained.push(worker.test_reserve_compaction_feedback().unwrap());
    drop(retained);

    let harness = fixture
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let pause = harness.pause_compaction_custody(CompactionCustodyTestStage::AdmissionReserved);
    let request = support::worker(move || worker.request_manual_compaction(&eligibility).unwrap());
    pause.wait_until_paused();
    drop(publication);
    pause.release();
    let feedback = request.join().unwrap();
    assert_eq!(
        feedback.snapshot().state,
        ContextCompactionFeedbackState::Rejected
    );
    assert_eq!(fixture.home().home_revision().unwrap(), before);
}

async fn wait_context(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    expected: Option<u8>,
    cx: &mut AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.update_window(window.into(), |_, native, app| {
            native.draw_and_present_for_test(app)
        })
        .unwrap();
        let matched = window
            .read_with(cx, |root, _| {
                root.test_context_remaining_percent() == expected
                    && root.test_context_rendered().is_some_and(|(value, bounds)| {
                        value == expected
                            && bounds.size.width == gpui::px(180.)
                            && bounds.size.height > gpui::px(0.)
                    })
            })
            .unwrap();
        if matched {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native context did not render {expected:?}"
        );
        native::pump(cx).await;
    }
}

#[test]
fn native_context_controls_render_usage_and_dispatch_exact_manual_compaction() {
    run_native_context(false, false, false);
}

#[test]
fn native_compaction_rejects_original_idle_capability_when_queued_work_wins() {
    run_native_context(true, false, false);
}

#[test]
fn native_compaction_rejects_original_claim_after_durable_selection_removal() {
    run_native_context(true, true, false);
}

#[test]
fn native_compaction_retains_indeterminate_admission_until_exact_retirement() {
    run_native_context(false, false, true);
}

fn run_native_context(queued_work: bool, claim_loss: bool, uncertainty: bool) {
    let faults = beryl_home_store::test_faults::FaultController::new();
    let fixture_faults = faults.clone();
    let server = server::NormalTerminalServer::spawn_controlled_connection_loss();
    let endpoint = server.endpoint();
    let (fixture, prepared, acquisition) = support::worker(move || {
        let mut fixture = syndic::Fixture::with_faults(197, fixture_faults);
        eprintln!("native context fixture: {}", fixture.home_path().display());
        let theme = appearance::system_font_appearance(&fixture.state);
        let (prepared, acquisition) = support::prepare_shell_with_theme(&mut fixture, theme);
        let home = fixture.home();
        let settings = fixture.state.settings();
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(
                settings.apply(
                    settings.revision(&home).unwrap(),
                    beryl_state::ApplySettings::new(vec![beryl_state::SettingUpdate::new(
                        beryl_state::SettingKey::ContextCompactionTimeout,
                        beryl_state::ExpectedSettingRevision::Absent,
                        beryl_state::SettingValue::context_compaction_timeout_millis(1_000),
                    )])
                    .unwrap(),
                ),
            )
            .unwrap();
        assert!(matches!(
            home.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        drop(home);
        (fixture, prepared, acquisition)
    })
    .join()
    .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            gpui_text_input::ensure_text_input_bindings(app);
            let owner = beryl_app::theme_runtime::GpuiAppearanceWindowSet::new(
                prepared.appearance().clone(),
                std::num::NonZeroUsize::new(4).unwrap(),
                app,
            );
            let mut shell = GpuiMainWindowShellHost::new(app, owner)
                .construct_hidden(prepared)
                .unwrap_or_else(|_| panic!("native context shell"));
            let window = shell.window();
            let worker = fixture.store.exact_stop_worker();
            let publication = Arc::new(());
            window
                .update(app, |root, window, app| {
                    root.test_mount_exact_status_worker(
                        worker.clone(),
                        Arc::downgrade(&publication),
                        fixture.state.session(),
                        window,
                        app,
                    )
                })
                .unwrap();
            app.spawn(async move |cx| {
                let deadline = Instant::now() + Duration::from_secs(15);
                while !cx.update(|app| shell.ready_to_publish(app)).unwrap() {
                    assert!(
                        Instant::now() < deadline,
                        "native context shell not presentable"
                    );
                    window.update(cx, |_, window, _| window.refresh()).unwrap();
                    native::pump(cx).await;
                }
                cx.update(|app| shell.publish(app)).unwrap().unwrap();
                window
                    .update(cx, |_, window, _| {
                        window.minimize_window();
                    })
                    .unwrap();
                wait_context(window, None, cx).await;
                let (fixture, mut session, projection, coordinator) = join(
                    support::worker(move || {
                        let mut fixture = fixture;
                        fixture.submit_text(server::SUBMITTED_TEXT);
                        let connector = ManagedBackendClientConnector::for_lifecycle_test(
                            endpoint,
                            server::AUTHORIZATION,
                        );
                        let mut session = fixture
                            .store
                            .admit_runtime_lifecycle_test_candidate(
                                &connector,
                                syndic::execution_binding(),
                                CasProcessGeneration::new(48_197).unwrap(),
                                std::path::Path::new(EXECUTION_ROOT),
                                server::TIMEOUT,
                            )
                            .unwrap();
                        let coordinator =
                            CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
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
                )
                .await;
                server.wait_for_projection();
                let selected_thread = fixture.thread;
                let capture = support::worker(move || {
                    let outcome = coordinator.execute_ordinary_turn(
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
                    );
                    (fixture, outcome)
                });
                let deadline = Instant::now() + Duration::from_secs(10);
                while !window
                    .read_with(cx, |root, _| {
                        root.test_exact_status_diagnostics().0 == "working"
                    })
                    .unwrap()
                {
                    assert!(
                        Instant::now() < deadline,
                        "native context turn did not start"
                    );
                    native::pump(cx).await;
                }
                server.send_observation(wire(50));
                wait_context(window, Some(75), cx).await;
                server.send_observation(wire(-1));
                wait_context(window, None, cx).await;
                server.send_observation(wire(25));
                wait_context(window, Some(87), cx).await;
                let earlier_stop = join(support::worker({
                    let worker = worker.clone();
                    let thread = selected_thread;
                    move || {
                        let ExactSoftStopAvailability::Eligible(eligibility) = worker.exact_soft_stop_eligibility(thread)
                            else { panic!("original stop feedback needs live operation"); };
                        eligibility.test_projected_feedback(ExactStopFeedbackState::Completed).unwrap()
                    }
                }), cx).await;
                server.send_observation(server::terminal_wire());
                let (fixture, outcome) = join(capture, cx).await;
                assert!(matches!(
                    outcome,
                    Ok(OrdinaryTurnExecutionOutcome::Terminal { .. })
                ));
                wait_context(window, Some(87), cx).await;
                let deadline = Instant::now() + Duration::from_secs(10);
                while !window
                    .read_with(cx, |root, _| root.test_exact_status_diagnostics().0 == "ok")
                    .unwrap()
                {
                    assert!(
                        Instant::now() < deadline,
                        "native context terminal state did not settle"
                    );
                    native::pump(cx).await;
                }
                let deadline = Instant::now() + Duration::from_secs(10);
                while !window.read_with(cx, |root, _| root.test_compaction_command_enabled()).unwrap() {
                    assert!(Instant::now() < deadline, "native Compact capability did not become available");
                    native::pump(cx).await;
                }
                let fixture = join(support::worker(move || {
                    verify_manual_admission_fences(&fixture);
                    fixture
                }), cx).await;
                let (fixture, server) = if queued_work {
                    let fixture = join(support::worker(move || {
                        let mut fixture = fixture;
                        let worker = fixture.store.exact_stop_worker();
                        let state = fixture.state.session();
                        let selection = state.minimal_bootstrap(&fixture.home()).unwrap().unwrap().windows()[0].selected_thread().unwrap();
                        let lifetime = Arc::new(());
                        let ManualCompactionAvailability::Eligible(eligibility) = worker.test_selected_compaction_availability(
                            &state, WindowId::from_bytes([190; 16]), selection, Arc::downgrade(&lifetime),
                        ) else { panic!("queued race needs original idle authority"); };
                        let pause = fixture.store.context_compaction_lifecycle_test_harness().unwrap()
                            .pause_compaction_custody(CompactionCustodyTestStage::AdmissionReserved);
                        let request = support::worker(move || worker.request_manual_compaction(&eligibility).unwrap());
                        pause.wait_until_paused();
                        if claim_loss {
                            let home = fixture.home();
                            let bootstrap = state.minimal_bootstrap(&home).unwrap().unwrap();
                            let record = &bootstrap.windows()[0];
                            let mut command = HomeCommand::new(home.home_revision().unwrap());
                            command.add(state.remove_window(state.revision(&home).unwrap(), beryl_state::RemoveSessionWindow::new(
                                bootstrap.header().revision(), record.window_id(), record.revision(), record.selected_thread(),
                            ))).unwrap();
                            assert!(matches!(home.execute(command), CommandOutcome::Committed { later_failure: None, .. }));
                        } else {
                            fixture.advance_clock(u64::try_from(std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()).unwrap() + 1_000);
                            fixture.submit_text("queued work takes precedence");
                        }
                        let revision = fixture.home().home_revision().unwrap();
                        pause.release();
                        assert_eq!(request.join().unwrap().snapshot().state, ContextCompactionFeedbackState::Rejected);
                        assert_eq!(fixture.home().home_revision().unwrap(), revision);
                        if !claim_loss { assert!(matches!(fixture.storage.compaction_admission_read(&fixture.home(), fixture.thread,
                            syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()).unwrap(),
                            syndic_storage::CompactionAdmissionRead::Ineligible(syndic_storage::CompactionAdmissionIneligibility::Busy { .. }))); }
                        fixture
                    }), cx).await;
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while window.read_with(cx, |root, _| root.test_compaction_command_enabled()).unwrap() {
                        assert!(Instant::now() < deadline, "queued work did not disable Compact");
                        native::pump(cx).await;
                    }
                    let bounds = window.read_with(cx, |root, _| root.test_context_rendered().unwrap().1).unwrap();
                    click(window, bounds, cx);
                    assert!(window.read_with(cx, |root, _| root.test_compaction_feedback_states()).unwrap().is_empty());
                    (fixture, server)
                } else {
                let (fixture, original_draft) = join(support::worker(move || {
                    let draft = fixture.storage.current_draft(&fixture.home(), fixture.thread, syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()).unwrap().unwrap();
                    (fixture, draft)
                }), cx).await;
                if !uncertainty {
                    window.update(cx, |root, _, _| root.test_begin_exact_stop_feedback_handoff(&earlier_stop)).unwrap();
                }
                let ingress = window.update(cx, |root, window, app| root.notice_ingress(window, app)).unwrap();
                cx.update(|app| {
                    for _ in 0..NOTICE_GENERAL_CAPACITY {
                        assert!(matches!(ingress.admit(NoticeRecord {
                            window_id: WindowId::from_bytes([190; 16]), condition: NoticeConditionId::new(), revision: 1,
                            kind: NoticeKind::Warning, content: NoticeContent::new(NoticeVariant::Warning, NoticeDismissal::Dismissible, "Ordinary warning", "Capacity pressure"),
                        }, app), NoticeAdmission::Admitted(_)));
                    }
                }).unwrap();
                let bounds = window.read_with(cx, |root, _| root.test_context_rendered().unwrap().1).unwrap();
                click(window, bounds, cx);
                let row = loop {
                    cx.update_window(window.into(), |_, native, app| native.draw_and_present_for_test(app)).unwrap();
                    if let Some(bounds) = window.read_with(cx, |root, _| root.test_compaction_row_bounds()).unwrap() { break bounds; }
                    assert!(Instant::now() < deadline, "native Compact row was not mounted");
                    native::pump(cx).await;
                };
                if uncertainty {
                    faults.fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
                }
                click(window, row, cx);
                let (fixture, server) = if uncertainty {
                    let fixture = join(support::worker(move || {
                        let deadline = Instant::now() + Duration::from_secs(10);
                        while fixture.home().pending_reconciliations().is_empty() {
                            assert!(Instant::now() < deadline, "manual admission did not retain exact reconciliation");
                            std::thread::yield_now();
                        }
                        assert_eq!(fixture.home().pending_reconciliations().len(), 1);
                        let syndic_storage::CompactionAdmissionRead::Existing(operation) = fixture.storage.compaction_admission_read(
                            &fixture.home(), fixture.thread, syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap(),
                        ).unwrap() else { panic!("indeterminate admission must retain its exact operation"); };
                        assert!(operation.dispatch_claim().is_none());
                        assert!(operation.request().is_none());
                        let worker = fixture.store.exact_stop_worker();
                        let retained = (0..70).map(|_| worker.test_reserve_compaction_feedback().unwrap()).collect::<Vec<_>>();
                        assert!(matches!(worker.test_reserve_compaction_feedback(), Err(ExactStopRequestError::Capacity)));
                        drop(retained);
                        fixture
                    }), cx).await;
                    native::pump(cx).await;
                    assert_eq!(window.read_with(cx, |root, _| root.test_compaction_feedback_states()).unwrap(), vec![ContextCompactionFeedbackState::Pending]);
                    assert_eq!(window.read_with(cx, |root, _| root.notice_projection().unwrap().content.dismissal).unwrap(), NoticeDismissal::Persistent);
                    let fixture = join(support::worker(move || {
                        fixture.store.context_compaction_lifecycle_test_harness().unwrap().request_shutdown().unwrap();
                        assert_eq!(fixture.home().pending_reconciliations().len(), 1);
                        let current = fixture.storage.current_draft(&fixture.home(), fixture.thread, syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()).unwrap().unwrap();
                        assert_eq!(current.draft(), original_draft.draft());
                        assert_eq!(current.root(), original_draft.root());
                        fixture
                    }), cx).await;
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while window.read_with(cx, |root, _| root.test_compaction_feedback_states()).unwrap() != vec![ContextCompactionFeedbackState::AuthorityLost] {
                        assert!(Instant::now() < deadline, "indeterminate feedback did not resolve exact retirement");
                        native::pump(cx).await;
                    }
                    let fixture = join(support::worker(move || {
                        let home = fixture.home();
                        let scopes = home.pending_reconciliations();
                        assert_eq!(scopes.len(), 1);
                        assert!(matches!(home.reconcile(&scopes[0]).unwrap(), beryl_home_store::ReconciliationResolution::ExactNew { .. }));
                        assert!(home.pending_reconciliations().is_empty());
                        drop(home);
                        fixture
                    }), cx).await;
                    assert_eq!(window.read_with(cx, |root, _| root.test_compaction_feedback_states()).unwrap(), vec![ContextCompactionFeedbackState::AuthorityLost]);
                    (fixture, server)
                } else {
                let server = join(support::worker(move || { server.accept_compaction(); server }), cx).await;
                let deadline = Instant::now() + Duration::from_secs(10);
                while window.read_with(cx, |root, _| root.test_compaction_feedback_states()).unwrap() != vec![ContextCompactionFeedbackState::StillRunning] {
                    assert!(Instant::now() < deadline, "native compaction did not retain StillRunning feedback");
                    native::pump(cx).await;
                }
                assert_eq!(window.read_with(cx, |root, _| root.test_exact_status_diagnostics().0).unwrap(), "compacting");
                assert_ne!(window.read_with(cx, |root, _| root.notice_projection().unwrap().kind).unwrap(), NoticeKind::ExactStopFeedback);
                window.update(cx, |root, _, app| root.test_retain_exact_status_feedback(earlier_stop, app)).unwrap();
                native::pump(cx).await;
                let earlier_notice = window.read_with(cx, |root, _| {
                    let notice = root.notice_projection().unwrap();
                    assert_eq!(notice.kind, NoticeKind::ExactStopFeedback);
                    assert_eq!(notice.content.title().as_str(), "Operation completed");
                    notice.token.clone()
                }).unwrap();
                cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(earlier_notice), app).unwrap()).unwrap();
                native::pump(cx).await;
                window.read_with(cx, |root, _| {
                    let notice = root.notice_projection().unwrap();
                    assert_eq!(notice.kind, NoticeKind::ExactStopFeedback);
                    assert_eq!(notice.content.dismissal, NoticeDismissal::Persistent);
                    assert_eq!(notice.content.title().as_str(), "Context compaction is still running");
                }).unwrap();
                drop(publication);
                let harness = fixture.store.context_compaction_lifecycle_test_harness().unwrap();
                let pause = harness.pause_after_manual_settlement().unwrap();
                server.send_observation(format!(r#"{{"method":"thread/status/changed","params":{{"threadId":"{}","status":{{"type":"idle"}}}}}}"#, server::CAS_THREAD_ID));
                server.send_observation(format!(r#"{{"method":"turn/completed","params":{{"threadId":"{}","turn":{{"id":"manual-compaction","items":[],"itemsView":"notLoaded","status":"completed","error":null,"startedAt":37020,"completedAt":37023,"durationMs":3}}}}}}"#, server::CAS_THREAD_ID));
                let pause = join(support::worker(move || { pause.wait_until_settled(); pause }), cx).await;
                let retirement_started = Arc::new(AtomicBool::new(false));
                let started = retirement_started.clone();
                let retirement = support::worker(move || {
                    started.store(true, Ordering::Release);
                    harness.request_shutdown().unwrap();
                });
                while !retirement_started.load(Ordering::Acquire) { native::pump(cx).await; }
                assert!(!retirement.is_finished(), "retirement must wait for exact result publication");
                pause.release();
                join(retirement, cx).await;
                let deadline = Instant::now() + Duration::from_secs(10);
                while window.read_with(cx, |root, _| root.test_compaction_feedback_states()).unwrap() != vec![ContextCompactionFeedbackState::Succeeded] {
                    assert!(Instant::now() < deadline, "native exact late settlement was lost after publication removal");
                    native::pump(cx).await;
                }
                let fixture = join(support::worker(move || {
                    let current = fixture.storage.current_draft(&fixture.home(), fixture.thread, syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()).unwrap().unwrap();
                    assert_eq!(current.draft(), original_draft.draft());
                    assert_eq!(current.root(), original_draft.root());
                    fixture
                }), cx).await;
                (fixture, server)
                };
                (fixture, server)
                };
                session.invalidate_connection();
                wait_context(window, None, cx).await;
                server.close_connection();
                drop(outcome);
                drop(session);
                server.join();
                window
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
                drop(shell);
                join(
                    support::worker(move || {
                        drop(acquisition);
                        let (directory, service) = fixture.into_service();
                        assert!(matches!(
                            service.close().unwrap(),
                            ProjectionConnectionServiceCloseOutcome::Closed
                        ));
                        drop(directory);
                    }),
                    cx,
                )
                .await;
                result.store(true, Ordering::Release);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        });
    assert!(completed.load(Ordering::Acquire));
}
