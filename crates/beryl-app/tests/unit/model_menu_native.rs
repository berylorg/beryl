use super::*;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, WM_LBUTTONDOWN, WM_LBUTTONUP};

#[test]
fn native_threadless_unknown_model_readout_is_passive_and_preserves_focus() {
    passive_unknown(0);
}

#[test]
fn native_selected_thread_without_known_model_does_not_open_or_probe_from_pointer_activation() {
    passive_unknown(1);
}

fn passive_unknown(count: u8) {
    run_mounted(
        count,
        0,
        move |owner, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let handle = native(window, cx).await;
                let before = window
                    .update(cx, |root, native, app| {
                        root.test_toggle_model_menu(native, app);
                        let (values, _, _, popup) = root.test_model_status();
                        assert_eq!(values.model, None);
                        assert_eq!(values.reasoning, None);
                        assert!(!popup);
                        native.focused(app)
                    })
                    .unwrap();
                let mut bounds = windows::Win32::Foundation::RECT::default();
                unsafe {
                    GetClientRect(handle, &mut bounds).unwrap();
                }
                let x = 80_u32;
                let y = bounds.bottom.saturating_sub(14) as u32;
                let position = LPARAM(((y << 16) | x) as isize);
                unsafe {
                    PostMessageW(Some(handle), WM_LBUTTONDOWN, WPARAM(1), position).unwrap();
                    PostMessageW(Some(handle), WM_LBUTTONUP, WPARAM(0), position).unwrap();
                }
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                window
                    .update(cx, |root, native, app| {
                        assert!(!root.test_model_status().3);
                        assert_eq!(native.focused(app), before);
                        assert!(root.test_model_menu_diagnostics().is_none());
                    })
                    .unwrap();
                activate_exit(window, cx);
            })
        },
        true,
        usize::from(count.max(1)),
    );
}

#[test]
fn native_progressive_model_menu_pointer_keyboard_reasoning_and_focus_use_real_admitted_pages() {
    use crate::model_selection::test_support::{
        AUTHORIZATION, ModelResponse, ProtocolServer, TIMEOUT,
    };
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::{VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_RETURN, VK_SPACE},
        WindowsAndMessaging::{WM_KEYDOWN, WM_KEYUP},
    };
    run_mounted(
        1,
        0,
        move |owner, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let editor = composer(window, cx).await;
                let selection = editor
                    .read_with(cx, |editor, _| editor.selection_identity())
                    .unwrap();
                let server = ProtocolServer::new();
                let connector = beryl_backend::ManagedBackendClientConnector::for_lifecycle_test(
                    server.endpoint(),
                    AUTHORIZATION,
                );
                let admitted = {
                    let retained = owner.borrow();
                    let graph = retained.test_services().graph().unwrap();
                    let execution = graph
                        .syndic()
                        .thread_execution(
                            graph.home(),
                            selection.claim().thread_id(),
                            syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
                        )
                        .unwrap()
                        .unwrap()
                        .execution()
                        .clone();
                    let generation = beryl_model::CasProcessGeneration::new(73_001).unwrap();
                    let admitted = graph
                        .cas()
                        .admit_runtime_lifecycle_test_candidate(
                            &connector,
                            execution.clone(),
                            generation,
                            std::path::Path::new(execution.root_path().as_str()),
                            TIMEOUT,
                        )
                        .unwrap();
                    graph
                        .cas()
                        .attach_model_connector_for_test(&execution, generation, connector);
                    admitted
                };
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    if window
                        .read_with(cx, |root, _| {
                            let (values, available, _, _) = root.test_model_status();
                            available && values.model.as_deref() == Some("actual-model")
                        })
                        .unwrap()
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "actual admitted default did not reach selected status"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                server.enqueue(ModelResponse::page_range(0, 64, Some("second")));
                server.enqueue(ModelResponse::page_range(64, 64, None));
                let prior_focus = window
                    .update(cx, |root, native, app| {
                        let focus = native.focused(app);
                        root.test_toggle_model_menu(native, app);
                        let state = root.test_model_menu_state().unwrap();
                        assert!(state.2);
                        assert_eq!(state.0, 0);
                        assert_eq!(root.test_model_status().0.reasoning, None);
                        focus
                    })
                    .unwrap();
                loop {
                    if window
                        .read_with(cx, |root, _| {
                            root.test_model_menu_state()
                                .is_some_and(|state| state.0 == 128 && !state.2)
                        })
                        .unwrap()
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "progressive menu did not publish both actual pages"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                let handle = native(window, cx).await;
                let menu = window
                    .update(cx, |root, native, _| {
                        let diagnostics = root.test_model_menu_diagnostics().unwrap();
                        assert!(diagnostics.1 <= 11);
                        assert_eq!(diagnostics.3, 0);
                        assert_eq!(root.test_model_menu_state().unwrap().1, 2);
                        let bounds = root.test_model_menu_bounds(native).unwrap();
                        assert!(bounds.top() >= gpui::px(0.));
                        assert!(bounds.bottom() <= native.viewport_size().height);
                        bounds
                    })
                    .unwrap();
                let x = f32::from(menu.left() + gpui::px(40.)) as u32;
                let y = f32::from(menu.top() + gpui::px(4. + 2. * 30. + 15.)) as u32;
                let position = LPARAM(((y << 16) | x) as isize);
                unsafe {
                    PostMessageW(Some(handle), WM_LBUTTONDOWN, WPARAM(1), position).unwrap();
                    PostMessageW(Some(handle), WM_LBUTTONUP, WPARAM(0), position).unwrap();
                }
                wait_model(window, "model-2", None, cx).await;
                let post_key = |key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe {
                    PostMessageW(
                        Some(handle),
                        WM_KEYDOWN,
                        WPARAM(usize::from(key.0)),
                        LPARAM(0),
                    )
                    .unwrap();
                    PostMessageW(
                        Some(handle),
                        WM_KEYUP,
                        WPARAM(usize::from(key.0)),
                        LPARAM(0),
                    )
                    .unwrap();
                };
                post_key(VK_HOME);
                post_key(VK_DOWN);
                post_key(VK_SPACE);
                wait_model(window, "model-1", None, cx).await;
                post_key(VK_END);
                post_key(VK_RETURN);
                wait_model(window, "model-1", Some("high"), cx).await;
                post_key(VK_ESCAPE);
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                window
                    .update(cx, |root, native, app| {
                        assert!(!root.test_model_status().3);
                        assert_eq!(native.focused(app), prior_focus);
                    })
                    .unwrap();
                assert_eq!(server.requests("model/list").len(), 2);
                assert!(server.requests("turn/start").is_empty());
                retire_model_runtime(&owner);
                drop(admitted);
                drop(server);
                exit_model_fixture(window, cx).await;
            })
        },
        true,
        1,
    );
}

async fn wait_model(
    window: WindowHandle<MainWindowShellRoot>,
    model: &str,
    reasoning: Option<&str>,
    cx: &mut gpui::AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if window
            .read_with(cx, |root, _| {
                let values = root.test_model_status().0;
                values.model.as_deref() == Some(model) && values.reasoning.as_deref() == reasoning
            })
            .unwrap()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native stable model/reasoning activation did not reach status"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

#[test]
fn native_initial_model_failure_retry_keeps_feedback_disables_duplicates_and_publishes_exact_empty()
{
    use crate::model_selection::test_support::ModelResponse;
    run_mounted(
        1,
        0,
        move |owner, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let (server, admitted, _) = install_model_runtime(&owner, window, cx).await;
                server.enqueue(ModelResponse::Failure);
                window
                    .update(cx, |root, native, app| {
                        root.test_toggle_model_menu(native, app)
                    })
                    .unwrap();
                wait_menu(window, cx, |state| {
                    state.0 == 0 && !state.2 && state.4 == Some("Models could not be loaded.")
                })
                .await;
                let held = server.hold(ModelResponse::page(0, None));
                window
                    .update(cx, |root, native, app| {
                        for _ in 0..4 {
                            root.test_retry_model_menu(native, app);
                        }
                        let state = root.test_model_menu_state().unwrap();
                        assert!(state.2 && state.3);
                        assert_eq!(state.4, Some("Models could not be loaded."));
                        assert_eq!(root.test_model_status().0.reasoning, None);
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while server.requests("model/list").len() != 2 {
                    assert!(
                        Instant::now() < deadline,
                        "exact retry did not reach provider"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                held.wait();
                assert_eq!(server.requests("model/list").len(), 2);
                held.release();
                wait_menu(window, cx, |state| {
                    state.0 == 0 && !state.2 && state.4.is_none()
                })
                .await;
                window
                    .update(cx, |root, native, app| {
                        assert_eq!(
                            root.test_model_status().0.model.as_deref(),
                            Some("actual-model")
                        );
                        assert_eq!(root.test_model_status().0.reasoning, None);
                        assert_eq!(root.test_model_menu_diagnostics().unwrap().0, 1);
                        root.test_toggle_model_menu(native, app);
                    })
                    .unwrap();
                retire_model_runtime(&owner);
                drop(admitted);
                drop(server);
                exit_model_fixture(window, cx).await;
            })
        },
        true,
        1,
    );
}

#[test]
fn native_later_model_failure_preserves_rows_and_choice_while_exact_retry_is_pending() {
    use crate::model_selection::{ModelReasoningEffort, test_support::ModelResponse};
    run_mounted(
        1,
        0,
        move |owner, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let (server, admitted, _) = install_model_runtime(&owner, window, cx).await;
                server.enqueue(ModelResponse::page_range(0, 64, Some("failed-second")));
                server.enqueue(ModelResponse::Failure);
                window
                    .update(cx, |root, native, app| {
                        root.test_toggle_model_menu(native, app)
                    })
                    .unwrap();
                wait_menu(window, cx, |state| {
                    state.0 == 64 && !state.2 && state.4 == Some("Model results are incomplete.")
                })
                .await;
                let held = server.hold(ModelResponse::page_range(64, 8, None));
                window
                    .update(cx, |root, native, app| {
                        root.test_retry_model_menu(native, app);
                        root.test_retry_model_menu(native, app);
                        root.test_choose_model(5, Some(ModelReasoningEffort::Low), native, app);
                        let state = root.test_model_menu_state().unwrap();
                        assert!(state.2 && state.3);
                        assert_eq!(state.0, 64);
                        assert_eq!(state.1, 1);
                        assert_eq!(state.4, Some("Model results are incomplete."));
                    })
                    .unwrap();
                wait_model(window, "model-5", Some("low"), cx).await;
                let deadline = Instant::now() + Duration::from_secs(5);
                while server.requests("model/list").len() != 3 {
                    assert!(
                        Instant::now() < deadline,
                        "later exact retry did not reach provider"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                held.wait();
                held.release();
                wait_menu(window, cx, |state| {
                    state.0 == 72 && !state.2 && state.4.is_none()
                })
                .await;
                wait_model(window, "model-5", Some("low"), cx).await;
                let requests = server.requests("model/list");
                assert_eq!(requests.len(), 3);
                assert_eq!(
                    requests[1]["params"]["cursor"],
                    requests[2]["params"]["cursor"]
                );
                window
                    .update(cx, |root, native, app| {
                        root.test_toggle_model_menu(native, app)
                    })
                    .unwrap();
                retire_model_runtime(&owner);
                drop(admitted);
                drop(server);
                exit_model_fixture(window, cx).await;
            })
        },
        true,
        1,
    );
}

#[test]
fn native_runtime_readiness_retirement_retires_old_popup_and_preserves_exact_cached_values() {
    use crate::model_selection::test_support::ModelResponse;
    run_mounted(
        1,
        0,
        move |owner, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let (server, admitted, execution) = install_model_runtime(&owner, window, cx).await;
                server.enqueue(ModelResponse::page_range(0, 8, None));
                window
                    .update(cx, |root, native, app| {
                        root.test_toggle_model_menu(native, app)
                    })
                    .unwrap();
                wait_menu(window, cx, |state| state.0 == 8 && !state.2).await;
                owner
                    .borrow()
                    .test_services()
                    .graph()
                    .unwrap()
                    .cas()
                    .retire_model_readiness_for_test(execution.runtime_id());
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if window
                        .read_with(cx, |root, _| !root.test_model_status().3)
                        .unwrap()
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "replaced runtime did not retire its old popup"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                window
                    .read_with(cx, |root, _| {
                        assert_eq!(
                            root.test_model_status().0.model.as_deref(),
                            Some("actual-model")
                        );
                        assert_eq!(root.test_model_status().0.reasoning, None);
                        assert!(root.test_model_menu_diagnostics().is_none());
                    })
                    .unwrap();
                drop(admitted);
                drop(server);
                exit_model_fixture(window, cx).await;
            })
        },
        true,
        1,
    );
}

async fn install_model_runtime(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::AsyncApp,
) -> (
    crate::model_selection::test_support::ProtocolServer,
    crate::cas_projection::AdmittedProjectionSession,
    beryl_model::ExecutionBinding,
) {
    install_model_runtime_inner(owner, window, cx, true).await
}

fn retire_model_runtime(owner: &Rc<RefCell<RunningProcessOwner>>) {
    let retained = owner.borrow();
    let graph = retained.test_services().graph().unwrap();
    let selected = graph
        .state()
        .session()
        .minimal_bootstrap(graph.home())
        .unwrap()
        .unwrap();
    let thread = selected.windows()[0].selected_thread().unwrap().thread_id();
    let execution = graph
        .syndic()
        .thread_execution(
            graph.home(),
            thread,
            syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    graph
        .cas()
        .retire_model_readiness_for_test(execution.execution().runtime_id());
}

async fn exit_model_fixture(window: WindowHandle<MainWindowShellRoot>, cx: &mut gpui::AsyncApp) {
    let handle = native(window, cx).await;
    activate_exit(window, cx);
    native_support::confirm_fixture_exit(handle, cx).await;
}

async fn install_model_runtime_inner(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::AsyncApp,
    wait_for_draft_default: bool,
) -> (
    crate::model_selection::test_support::ProtocolServer,
    crate::cas_projection::AdmittedProjectionSession,
    beryl_model::ExecutionBinding,
) {
    use crate::model_selection::test_support::{AUTHORIZATION, ProtocolServer, TIMEOUT};
    let editor = composer(window, cx).await;
    let selection = editor
        .read_with(cx, |editor, _| editor.selection_identity())
        .unwrap();
    let server = ProtocolServer::new();
    let connector = beryl_backend::ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        AUTHORIZATION,
    );
    let (admitted, execution) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        let execution = graph
            .syndic()
            .thread_execution(
                graph.home(),
                selection.claim().thread_id(),
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap()
            .execution()
            .clone();
        let generation = beryl_model::CasProcessGeneration::new(73_001).unwrap();
        let admitted = graph
            .cas()
            .admit_runtime_lifecycle_test_candidate(
                &connector,
                execution.clone(),
                generation,
                std::path::Path::new(execution.root_path().as_str()),
                TIMEOUT,
            )
            .unwrap();
        graph
            .cas()
            .attach_model_connector_for_test(&execution, generation, connector);
        (admitted, execution)
    };
    if !wait_for_draft_default {
        return (server, admitted, execution);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if window
            .read_with(cx, |root, _| {
                let (values, available, _, _) = root.test_model_status();
                available && values.model.as_deref() == Some("actual-model")
            })
            .unwrap()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual admitted default did not reach selected status"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    (server, admitted, execution)
}

async fn wait_menu(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::AsyncApp,
    ready: impl Fn((usize, usize, bool, bool, Option<&'static str>, usize)) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if window
            .read_with(cx, |root, _| {
                root.test_model_menu_state().is_some_and(&ready)
            })
            .unwrap()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native model menu did not reach expected feedback/publication state"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

#[test]
fn native_idle_elected_model_change_replays_evicted_selection_and_preserves_logical_focus() {
    use crate::model_selection::{ModelReasoningEffort, test_support::ModelResponse};
    run_mounted(
        1,
        0,
        move |owner, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let (server, admitted, _) = install_model_runtime(&owner, window, cx).await;
                server.enqueue(ModelResponse::page_range(0, 64, Some("second")));
                server.enqueue(ModelResponse::page_range(64, 64, Some("third")));
                server.enqueue(ModelResponse::page_range(128, 64, None));
                window
                    .update(cx, |root, native, app| {
                        root.test_toggle_model_menu(native, app)
                    })
                    .unwrap();
                wait_menu(window, cx, |state| state.0 == 192 && !state.2).await;
                window
                    .read_with(cx, |root, _| {
                        assert_eq!(root.test_model_menu_focused_row(), Some("id-0"))
                    })
                    .unwrap();
                let editor = composer(window, cx).await;
                let selection = editor
                    .read_with(cx, |editor, _| editor.selection_identity())
                    .unwrap();
                let publication = owner
                    .borrow()
                    .test_services()
                    .model_selection_reader()
                    .unwrap();
                server.enqueue(ModelResponse::page_range(80, 1, None));
                server.enqueue(ModelResponse::page_range(0, 64, Some("second")));
                server.enqueue(ModelResponse::page_range(64, 64, Some("third")));
                server.enqueue(ModelResponse::page_range(128, 64, None));
                server.enqueue(ModelResponse::page_range(64, 64, Some("third")));
                cx.background_executor()
                    .spawn(async move {
                        let old_status = publication.observe(selection).unwrap();
                        let scope = publication.prepare(selection).unwrap();
                        let page = scope.query.read_page(None).unwrap();
                        publication
                            .choose(
                                &scope,
                                selection,
                                &page,
                                &page.records()[0],
                                Some(ModelReasoningEffort::Low),
                            )
                            .unwrap();
                        assert!(old_status.query.with_current_status(|| ()).is_ok());
                        let mut published = false;
                        assert!(old_status.with_current_status(|| published = true).is_err());
                        assert!(!published);
                    })
                    .await;
                wait_model(window, "model-80", Some("low"), cx).await;
                wait_menu(window, cx, |state| state.5 == 80 && !state.2).await;
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    let revealed = window
                        .read_with(cx, |root, _| {
                            let diagnostics = root.test_model_menu_diagnostics().unwrap();
                            assert!(diagnostics.1 <= 11);
                            assert_eq!(root.test_model_menu_focused_row(), Some("id-0"));
                            assert_eq!(root.test_model_menu_state().unwrap().1, 2);
                            diagnostics.2.contains(&80)
                        })
                        .unwrap();
                    if revealed {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "evicted exact selected model was not revealed"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                assert_eq!(server.requests("model/list").len(), 8);
                window
                    .update(cx, |root, native, app| {
                        root.test_toggle_model_menu(native, app)
                    })
                    .unwrap();
                retire_model_runtime(&owner);
                drop(admitted);
                drop(server);
                exit_model_fixture(window, cx).await;
            })
        },
        true,
        1,
    );
}

struct StatusSessionTools;
impl crate::cas_projection::OrdinaryDynamicToolAuthority for StatusSessionTools {
    fn handlers(&mut self) -> crate::cas_projection::OrdinaryDynamicToolHandlers<'_> {
        panic!("loaded metadata qualification must not dispatch a dynamic tool")
    }
}

fn prepare_completed_status_home(path: &std::path::Path) {
    use crate::support::exact_cas::{
        admit_event, converge_and_release_terminal_history, correlate_user_item,
        establish_turn_with_profile, submit_current_draft,
    };
    use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion};
    use beryl_model::{SyndicDraftId, SyndicItemId};
    use syndic_storage::{SourceEventPayload, SyndicTimestamp, TurnEndStatus};
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let storage = syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(
            beryl_state::BerylState::required_domains()
                .unwrap()
                .merge(syndic_storage::SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let selected = state.session().minimal_bootstrap(&home).unwrap().unwrap();
    let thread = selected.windows()[0].selected_thread().unwrap().thread_id();
    let item = SyndicItemId::from_bytes([241; 16]);
    let turn = submit_current_draft(
        &home,
        storage.clone(),
        thread,
        SyndicDraftId::from_bytes([242; 16]),
        item,
        "Completed metadata history",
        SyndicTimestamp::from_unix_millis(101),
    );
    let profile = crate::conversation_tools::ConversationToolRegistry::canonical().profile();
    let source = establish_turn_with_profile(
        &home,
        storage.clone(),
        thread,
        turn,
        SyndicTimestamp::from_unix_millis(102),
        profile,
    );
    admit_event(
        &home,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        SyndicTimestamp::from_unix_millis(103),
    );
    correlate_user_item(
        &home,
        storage.clone(),
        thread,
        turn,
        item,
        &source,
        SyndicTimestamp::from_unix_millis(104),
    );
    admit_event(
        &home,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        SyndicTimestamp::from_unix_millis(105),
    );
    converge_and_release_terminal_history(&home, storage.clone(), thread, turn);
    let completed = storage
        .current_binding(
            &home,
            thread,
            syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let syndic_storage::BindingState::Valid(binding) = completed.binding().state() else {
        panic!("completed metadata history must retain a valid native binding");
    };
    assert_eq!(binding.tool_profile(), profile);
    assert_eq!(binding.cas_thread_id(), source.thread_id());
    home.close().unwrap();
}

#[test]
fn native_completed_thread_status_uses_authentic_loaded_session_metadata_without_pending_choice() {
    use crate::cas_projection::{
        CasProjectionCoordinator, CasProjectionRequest, ProjectionCancellationToken,
        ScheduledOrdinaryRequestPolicy,
    };
    use crate::model_selection::test_support::{ModelResponse, TIMEOUT};
    use beryl_backend::ThreadStartOptions;
    use syndic_storage::{BindingState, SyndicPointReadLimit, SyndicTimestamp};
    let final_selection = Rc::new(RefCell::new(None));
    let observed_selection = final_selection.clone();
    run_mounted_with_prepared_home(
        1,
        0,
        prepare_completed_status_home,
        final_selection,
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let (server, mut admitted, execution) =
                    install_model_runtime_inner(&owner, window, cx, false).await;
                let editor = composer(window, cx).await;
                let selection = editor
                    .read_with(cx, |editor, _| editor.selection_identity())
                    .unwrap();
                let thread = selection.claim().thread_id();
                *observed_selection.borrow_mut() = Some((selection.window_id(), thread));
                let (loaded, registration) = {
                    let retained = owner.borrow();
                    let graph = retained.test_services().graph().unwrap();
                    let home = graph.home();
                    let storage = graph.syndic();
                    let limit = SyndicPointReadLimit::new(65_536).unwrap();
                    let record = storage.thread(home, thread, limit).unwrap().unwrap();
                    assert!(record.committed_tail().is_some());
                    let binding = storage
                        .current_binding(home, thread, limit)
                        .unwrap()
                        .unwrap();
                    let BindingState::Valid(binding) = binding.binding().state() else {
                        panic!("completed history must retain actual resumable CAS binding");
                    };
                    server.enqueue(ModelResponse::resumed_thread(
                        binding.cas_thread_id().as_str(),
                        execution.root_path().as_str(),
                        "loaded-model",
                        Some("high"),
                    ));
                    let loaded = CasProjectionCoordinator::for_healthy_home(home)
                        .unwrap()
                        .obtain_completed_projection_for_test(
                            home,
                            storage,
                            &mut admitted,
                            &CasProjectionRequest::new(
                                thread,
                                record.selected_path(),
                                execution.clone(),
                                ThreadStartOptions::persistent(),
                                Some(1_000_000),
                                SyndicTimestamp::from_unix_millis(106),
                                TIMEOUT,
                            ),
                            &ProjectionCancellationToken::new(),
                        )
                        .unwrap();
                    assert!(
                        graph
                            .sessions()
                            .pending_model_choice(thread, &execution)
                            .is_none()
                    );
                    let policy = ScheduledOrdinaryRequestPolicy::backend_defaults(
                        graph.state().settings(),
                        Some(1_000_000),
                        TIMEOUT,
                        TIMEOUT,
                    );
                    let registration = graph
                        .sessions()
                        .register(
                            thread,
                            execution.clone(),
                            admitted,
                            policy,
                            graph.state().assets(),
                            Box::new(StatusSessionTools),
                        )
                        .unwrap();
                    (loaded, registration)
                };
                wait_model(window, "loaded-model", Some("high"), cx).await;
                {
                    let retained = owner.borrow();
                    let graph = retained.test_services().graph().unwrap();
                    assert!(
                        graph
                            .sessions()
                            .pending_model_choice(thread, &execution)
                            .is_none()
                    );
                    let record = graph
                        .syndic()
                        .thread(
                            graph.home(),
                            thread,
                            SyndicPointReadLimit::new(65_536).unwrap(),
                        )
                        .unwrap()
                        .unwrap();
                    assert!(record.committed_tail().is_some());
                }
                assert_eq!(server.requests("thread/resume").len(), 1);
                assert!(server.requests("thread/start").is_empty());
                assert!(server.requests("turn/start").is_empty());
                assert!(server.requests("model/list").is_empty());
                window
                    .read_with(cx, |root, _| {
                        let (_, available, reason, popup) = root.test_model_status();
                        assert!(available);
                        assert_eq!(reason, None);
                        assert!(!popup);
                    })
                    .unwrap();
                drop(loaded);
                retire_model_runtime(&owner);
                owner
                    .borrow()
                    .test_services()
                    .graph()
                    .unwrap()
                    .sessions()
                    .retire(registration);
                drop(server);
                exit_model_fixture(window, cx).await;
            })
        },
        true,
        1,
    );
}
