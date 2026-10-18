use super::*;
use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{RootId, RuntimeId, WindowId};
use beryl_state::{BerylState, RememberedTarget};
use syndic_storage::*;

#[path = "../../support/native_shell_appearance.rs"]
mod appearance;
#[path = "../../syndic_composer_host/support.rs"]
mod draft;

pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub members: Vec<(WindowId, beryl_model::SyndicThreadId, String)>,
    pub home_id: beryl_model::BerylHomeId,
    pub runtime: RuntimeId,
}

pub(super) fn open(path: &Path) -> HomeOpenOutcome {
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let syndic = SyndicStorage::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    }
}

pub(super) fn selected(count: u8, large: bool, replacement: bool) -> Fixture {
    let (directory, first_window, first_thread) = super::super::creation_context::selected_home();
    let HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    } = open(directory.path())
    else {
        unreachable!()
    };
    let store = candidate.publish().unwrap();
    appearance::install_native_theme(&store, &state);
    let mut members = vec![(first_window, first_thread, String::new())];
    let registry =
        crate::window_acquisition::RuntimeBackedWindowProcessRegistry::new(Default::default());
    let service = crate::window_acquisition::RuntimeBackedWindowAcquisitionService::new(
        &registry,
        Arc::new(store.service_reference()),
        state.clone(),
        syndic.clone(),
    );
    for index in 1..count {
        let window = WindowId::from_bytes([94 + index; 16]);
        let reference = store.service_reference();
        let request = (inputs::windows().request_source)(
            window,
            RememberedTarget::new(
                RuntimeId::from_bytes([91; 16]),
                RootId::from_bytes([92; 16]),
            ),
            crate::main_window::MainWindowCreationRequestContext::new(&reference, &state),
        )
        .unwrap();
        let crate::window_acquisition::RuntimeBackedWindowAcquisitionOutcome::Committed {
            acquisition,
            ..
        } = service.acquire(request, CommandCancellation::new())
        else {
            panic!("fixture acquisition must commit")
        };
        members.push((window, acquisition.thread_id(), String::new()));
        drop(acquisition);
    }
    for (index, (_, thread, text)) in members.iter_mut().enumerate() {
        *text = if large {
            format!(
                "{}{}\n",
                char::from(b'a' + index as u8),
                "\u{0301}".repeat(256)
            )
            .repeat(48)
        } else {
            format!("saved draft {}", index + 1)
        };
        seed(&store, &syndic, *thread, text, 150 + index as u8 * 10);
    }
    if replacement {
        let snapshot = state.session().minimal_bootstrap(&store).unwrap().unwrap();
        assert_eq!(snapshot.windows().len(), 1);
        let member = &snapshot.windows()[0];
        #[path = "../../support/session_window_commands.rs"]
        mod session_window_commands;
        draft::committed(session_window_commands::execute(
            &store,
            state
                .session()
                .remove_window(
                    &store,
                    state.session().revision(&store).unwrap(),
                    beryl_state::RemoveSessionWindow::new(
                        snapshot.header().revision(),
                        member.window_id(),
                        member.revision(),
                        member.selected_thread(),
                    ),
                )
                .unwrap(),
        ));
        members.clear();
    }
    let home_id = store.home_id();
    drop((service, registry, state, syndic));
    store.close().unwrap();
    Fixture {
        directory,
        members,
        home_id,
        runtime: RuntimeId::from_bytes([91; 16]),
    }
}

fn seed(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    text: &str,
    key: u8,
) {
    let before = draft::current(storage.clone(), store, thread);
    let transaction = draft::transaction(
        storage.clone(),
        store,
        &before,
        key,
        key + 1,
        vec![DraftPieceReplacementV1::new(
            draft::point(0),
            draft::point(0),
            vec![DraftPieceV1::Text(text.into())],
        )],
        draft::point(0),
    );
    draft::run_transaction(storage.clone(), store, &transaction, 10);
    let DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
        .draft_editor_candidate_session(
            store,
            before.draft().id(),
            DraftEditorCandidateSessionIdV1::from_bytes([key; 16]),
        )
        .unwrap()
    else {
        panic!("seed owns its actual candidate session")
    };
    let selector = DraftEditorCurrentSelectorV1::new(
        before.thread().id(),
        before.thread().revision(),
        before.draft().id(),
        before.draft().revision(),
        before.draft().piece_root(),
        before.draft().history(),
    );
    assert_eq!(
        session.newest_root().marker_commitment(),
        selector.root().marker_commitment()
    );
    assert_eq!(session.newest_root().marker_commitment().marker_count(), 0);
    assert_eq!(
        session
            .newest_root()
            .summary()
            .logical_extent()
            .logical_utf8_bytes(),
        text.len() as u64
    );
    let request = DraftEditorCandidatePublicationRequestV1::new(
        selector,
        session.session_id(),
        DraftPieceOperationIdV1::from_bytes([key + 3; 16]),
        session.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(session.newest_root(), session.newest_history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        before.draft().updated_at(),
    );
    let source = storage
        .capture_draft_editor_candidate_publication_source(
            store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                selector,
                DraftEditorCandidateActivationBindingV1::from_head(&session),
                request.operation_id(),
                request.published_at(),
            ),
        )
        .unwrap();
    let prepared = storage
        .prepare_draft_editor_candidate_publication(store, source, request.evidence())
        .unwrap();
    let outcome = draft::execute(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
    );
    assert!(
        matches!(
            &outcome,
            beryl_home_store::CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "seed publication retains its original outcome: {outcome:?}"
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let DraftEditorCandidateSessionReadOutcomeV1::Active(published) = storage
        .draft_editor_candidate_session(store, session.draft_id(), session.session_id())
        .unwrap()
    else {
        panic!("seed publication retains its exact active candidate session")
    };
    let disposal = storage
        .prepare_dispose_draft_editor_candidate_session(
            store,
            DraftEditorCandidateSessionDisposeRequestV1::new(
                published.draft_id(),
                published.session_id(),
                DraftPieceOperationIdV1::from_bytes([key + 4; 16]),
                published.session_generation(),
                DraftRootHistoryPairV1::new(
                    published.published_root(),
                    published.published_history(),
                ),
            ),
        )
        .unwrap();
    let outcome = draft::execute(
        store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(store).unwrap(),
            disposal.clone(),
        ),
    );
    assert!(
        matches!(
            &outcome,
            beryl_home_store::CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "seed disposal retains its original outcome: {outcome:?}"
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(store, &disposal, outcome)
            .unwrap(),
        DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
            | DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(_)
    ));
}

pub(super) fn threadless() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    } = open(directory.path())
    else {
        unreachable!()
    };
    let store = candidate.publish().unwrap();
    appearance::install_native_theme(&store, &state);
    let home_id = store.home_id();
    drop((state, syndic));
    store.close().unwrap();
    Fixture {
        directory,
        members: Vec::new(),
        home_id,
        runtime: RuntimeId::from_bytes([91; 16]),
    }
}

pub(super) fn configuration(fixture: &Fixture) -> Configuration {
    Configuration::new(
        fixture.directory.path().to_owned(),
        Arc::new(|path, _| open(path)),
        false,
    )
    .unwrap()
}

fn changed_placement() -> beryl_model::WindowPlacement {
    beryl_model::WindowPlacement::new(
        beryl_model::WindowBounds::new(40, 50, 800, 600).unwrap(),
        beryl_model::WindowDisplayState::Normal,
        None,
        None,
    )
}

pub(super) fn change_saved_placement(store: &HomeStore) {
    let state = BerylState::reacquire(store).unwrap();
    let snapshot = state.session().minimal_bootstrap(store).unwrap().unwrap();
    let member = &snapshot.windows()[0];
    assert_ne!(member.placement(), &changed_placement());
    draft::committed(draft::execute(
        store,
        state.session().update_placement(
            state.session().revision(store).unwrap(),
            beryl_state::UpdateWindowPlacement::new(
                snapshot.header().revision(),
                member.window_id(),
                member.revision(),
                changed_placement(),
            ),
        ),
    ));
}

pub(super) fn assert_changed_placement(fixture: &Fixture) {
    let HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    } = open(fixture.directory.path())
    else {
        unreachable!()
    };
    let store = candidate.publish().unwrap();
    let snapshot = state.session().minimal_bootstrap(&store).unwrap().unwrap();
    assert_eq!(snapshot.windows()[0].placement(), &changed_placement());
    drop((state, syndic));
    store.close().unwrap();
}

pub(super) fn assert_saved(fixture: &Fixture, edits: usize) {
    assert!(edits <= 2);
    let HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    } = open(fixture.directory.path())
    else {
        unreachable!()
    };
    let store = candidate.publish().unwrap();
    let snapshot = state.session().minimal_bootstrap(&store).unwrap().unwrap();
    assert_eq!(snapshot.windows().len(), fixture.members.len());
    for (window, thread, text) in &fixture.members {
        let member = snapshot
            .windows()
            .iter()
            .find(|member| member.window_id() == *window)
            .unwrap();
        assert_eq!(member.selected_thread().unwrap().thread_id(), *thread);
        let current = syndic
            .current_draft_piece_text_demand(
                &store,
                *thread,
                DraftPieceTextDemandV1::Forward(0),
                65536,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            current.value().bytes(),
            format!("{}{text}", "!".repeat(edits)).as_bytes()
        );
        assert!(
            syndic
                .history_summary(&store, *thread, SyndicPointReadLimit::new(65536).unwrap())
                .unwrap()
                .unwrap()
                .committed_tail()
                .is_none(),
            "local native input never creates a submitted model turn"
        );
    }
    drop((state, syndic));
    store.close().unwrap();
}

pub(super) async fn wait(
    cx: &mut gpui::AsyncApp,
    mut condition: impl FnMut(&mut gpui::App) -> bool,
    reason: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        if cx.update(&mut condition).unwrap() {
            return;
        }
        assert!(Instant::now() < deadline, "{reason}");
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
}

pub(super) async fn wait_with_failure_context(
    cx: &mut gpui::AsyncApp,
    mut condition: impl FnMut(&mut gpui::App) -> bool,
    reason: &str,
    mut failure_context: impl FnMut(&mut gpui::App) -> String,
) {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        if cx.update(&mut condition).unwrap() {
            return;
        }
        assert!(Instant::now() < deadline, "{reason}: {}", {
            let mut end = reason.len().min(96);
            while !reason.is_char_boundary(end) {
                end -= 1;
            }
            eprintln!(
                "native_text_wait_timeout snapshot_entry reason={}",
                &reason[..end]
            );
            cx.update(&mut failure_context)
                .unwrap_or_else(|_| "GUI failure context unavailable".to_owned())
        },);
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
}

pub(super) fn shells(
    app: &gpui::App,
) -> Vec<gpui::WindowHandle<crate::main_window::MainWindowShellRoot>> {
    app.windows()
        .into_iter()
        .filter_map(|window| window.downcast())
        .collect()
}

pub(super) fn native(
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    app: &mut gpui::App,
) -> windows::Win32::Foundation::HWND {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    window
        .update(app, |_, native, _| {
            let raw =
                HasWindowHandle::window_handle(native).expect("original native window is live");
            let RawWindowHandle::Win32(raw) = raw.as_raw() else {
                panic!("expected the original Windows native handle");
            };
            let handle = windows::Win32::Foundation::HWND(raw.hwnd.get() as *mut _);
            assert!(unsafe {
                windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(handle)).as_bool()
            });
            handle
        })
        .unwrap()
}

pub(super) fn type_character(
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    app: &mut gpui::App,
) {
    type_character_at(window, app, 0);
}

pub(super) fn type_character_at(
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    app: &mut gpui::App,
    caret: u64,
) {
    let handle = prepare_native_text_entry_at(window, app, caret);
    post_native_character(handle);
}

pub(super) fn prepare_native_text_entry_at(
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    app: &mut gpui::App,
    caret: u64,
) -> windows::Win32::Foundation::HWND {
    use windows::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};
    let handle = native(window, app);
    unsafe {
        ShowWindow(handle, SW_RESTORE);
        let _ = SetForegroundWindow(handle);
        let _ = SetActiveWindow(handle);
        let _ = SetFocus(Some(handle));
    }
    window
        .update(app, |root, native, app| {
            native.activate_window();
            root.test_submission_composer(app)
                .unwrap()
                .read(app)
                .gpui_input()
                .update(app, |input, app| {
                    let surface = input.surface().unwrap();
                    assert_eq!(
                        surface.source_caret().byte_offset.get(),
                        caret,
                        "normal insertion stays at its exact original source position"
                    );
                    assert_eq!(
                        surface.source_selection().anchor,
                        surface.source_selection().head
                    );
                    input.focus(native);
                    assert!(gpui::Focusable::focus_handle(input, app).is_focused(native));
                });
        })
        .unwrap();
    handle
}

pub(super) fn post_native_character(handle: windows::Win32::Foundation::HWND) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};

    assert_eq!(
        unsafe { GetActiveWindow() },
        handle,
        "native activation targets this GUI thread's ordinary shell"
    );
    assert_eq!(
        unsafe { GetFocus() },
        handle,
        "native focus targets this exact ordinary shell"
    );
    unsafe { PostMessageW(Some(handle), WM_CHAR, WPARAM(b'!' as usize), LPARAM(1)) }.unwrap();
}

pub(super) fn press_enter(
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    app: &mut gpui::App,
) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};
    let handle = native(window, app);
    unsafe {
        ShowWindow(handle, SW_RESTORE);
        let _ = SetForegroundWindow(handle);
        let _ = SetActiveWindow(handle);
        let _ = SetFocus(Some(handle));
    }
    window
        .update(app, |root, native, cx| {
            native.activate_window();
            let composer = root.test_submission_composer(cx).unwrap();
            let input = composer.read(cx).gpui_input();
            input.update(cx, |input, cx| {
                assert!(input.is_enabled());
                input.focus(native);
                assert!(gpui::Focusable::focus_handle(input, cx).is_focused(native));
            });
            assert_eq!(unsafe { GetActiveWindow() }, handle);
            assert_eq!(unsafe { GetFocus() }, handle);
            unsafe {
                PostMessageW(
                    Some(handle),
                    WM_KEYDOWN,
                    WPARAM(VK_RETURN.0 as usize),
                    LPARAM(1),
                )
                .unwrap();
                PostMessageW(
                    Some(handle),
                    WM_KEYUP,
                    WPARAM(VK_RETURN.0 as usize),
                    LPARAM(0xc000_0001_u32 as isize),
                )
                .unwrap();
            }
        })
        .unwrap();
}

pub(super) fn configure_autosave(
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    app: &mut gpui::App,
    generation: u64,
) {
    let interval = crate::composer_host::ComposerHostAutosaveInterval::new(5).unwrap();
    window
        .update(app, |root, native, cx| {
            let composer = root.test_submission_composer(cx).unwrap();
            let expected = if generation == 1 {
                crate::composer_host::ComposerHostAutosaveInterval::new(30).unwrap()
            } else {
                interval
            };
            assert_eq!(
                composer.read(cx).test_autosave_observation(),
                Some((expected, false, false)),
                "timing configuration names the clean original mounted owner"
            );
            let mount = root.controller().unwrap().composer_mount().unwrap();
            mount.update(cx, |mount, mount_cx| {
                assert!(matches!(
                    mount
                        .publish_autosave_interval(generation, interval, native, mount_cx)
                        .unwrap(),
                    crate::composer_host::ComposerHostAutosaveSettingsCompletion::Published(None)
                ));
            });
            assert_eq!(
                root.test_submission_composer(cx).unwrap().entity_id(),
                composer.entity_id(),
                "timing configuration preserves the same original editor"
            );
            assert_eq!(
                composer.read(cx).test_autosave_observation(),
                Some((interval, false, false)),
                "the actual original host uses the configured five-second interval"
            );
        })
        .unwrap();
}

pub(super) fn minimize(
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    app: &mut gpui::App,
) {
    let handle = native(window, app);
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::ShowWindow(
            handle,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWMINNOACTIVE,
        );
    }
}
