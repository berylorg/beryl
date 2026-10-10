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
fn native_context_cell_renders_exact_ordered_usage_and_retirement_unknown() {
    let server = server::NormalTerminalServer::spawn_controlled_connection_loss();
    let endpoint = server.endpoint();
    let (fixture, prepared, acquisition) = support::worker(|| {
        let mut fixture = syndic::Fixture::new(197);
        eprintln!("native context fixture: {}", fixture.home_path().display());
        let theme = appearance::system_font_appearance(&fixture.state);
        let (prepared, acquisition) = support::prepare_shell_with_theme(&mut fixture, theme);
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
                session.invalidate_connection();
                wait_context(window, None, cx).await;
                server.close_connection();
                drop(outcome);
                drop(session);
                server.join();
                drop(publication);
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
