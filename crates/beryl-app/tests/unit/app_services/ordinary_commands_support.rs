use super::*;

type Window = WindowHandle<MainWindowShellRoot>;
type Owner = Rc<RefCell<RunningProcessOwner>>;
pub(super) fn run_mounted(
    count: u8,
    chunks: usize,
    scenario: impl for<'a> FnOnce(
        Owner,
        &'a mut AsyncApp,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>
    + 'static,
    expect_shutdown: bool,
    restored_count: usize,
) -> MinimalSessionBootstrap {
    run_mounted_with_faults(
        count,
        chunks,
        move |owner, _, cx| scenario(owner, cx),
        expect_shutdown,
        restored_count,
    )
}

pub(super) fn run_mounted_with_faults(
    count: u8,
    chunks: usize,
    scenario: impl for<'a> FnOnce(
        Owner,
        FaultController,
        &'a mut AsyncApp,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>
    + 'static,
    expect_shutdown: bool,
    restored_count: usize,
) -> MinimalSessionBootstrap {
    run_mounted_case(
        count,
        chunks,
        scenario,
        expect_shutdown,
        Some(restored_count),
    )
    .unwrap()
}

pub(super) fn run_mounted_retained(
    count: u8,
    chunks: usize,
    scenario: impl for<'a> FnOnce(
        Owner,
        FaultController,
        &'a mut AsyncApp,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>
    + 'static,
) {
    assert!(run_mounted_case(count, chunks, scenario, false, None).is_none());
}

fn run_mounted_case(
    count: u8,
    chunks: usize,
    scenario: impl for<'a> FnOnce(
        Owner,
        FaultController,
        &'a mut AsyncApp,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>
    + 'static,
    expect_shutdown: bool,
    restored_count: Option<usize>,
) -> Option<MinimalSessionBootstrap> {
    let directory = if count == 0 {
        support::native_home()
    } else {
        resident_fixture::selected_home_with_draft(count, chunks)
    };
    eprintln!(
        "mounted ordinary command fixture: {}",
        directory.path().display()
    );
    let faults = FaultController::new();
    let opening_faults = faults.clone();
    let mut input = input(directory.path(), move |path, _| {
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            opening_faults.clone(),
        )
        .unwrap();
        let state = beryl_state::BerylState::register(&mut candidate).unwrap();
        let syndic = syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
        let candidate = candidate
            .prepare_publication(
                beryl_state::BerylState::required_domains()
                    .unwrap()
                    .merge(syndic_storage::SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        StartupHomeOpen::Ready {
            candidate,
            state,
            syndic,
        }
    });
    if count != 0 {
        input.windows = resident_fixture::selected_inputs_with_extent((chunks * 768) as u64);
        let execution = beryl_model::ExecutionBinding::new(
            beryl_model::RuntimeId::from_bytes([194; 16]),
            beryl_model::RootId::from_bytes([195; 16]),
            beryl_model::RuntimeNativePath::from_admitted(
                beryl_model::RuntimeMode::host(),
                beryl_model::PathFlavor::Windows,
                r"C:\Work\Beryl",
            )
            .unwrap(),
        );
        input.windows.request_source = Arc::new(move |window: WindowId, target| {
            let mut draft = *window.as_bytes();
            draft[0] ^= 0x80;
            crate::window_acquisition::RuntimeBackedWindowAcquisitionRequest::new(
                window,
                target,
                window_services::placement(),
                beryl_model::SyndicThreadId::from_bytes(*window.as_bytes()),
                beryl_model::SyndicDraftId::from_bytes(draft),
                execution.clone(),
                syndic_storage::SyndicTimestamp::from_unix_millis(1000),
                syndic_storage::DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
            )
            .map_err(|error| format!("{error:?}"))
        });
        input.windows.activation_source = Arc::new(|acquisition| {
            Ok((
                resident_fixture::composer_support::activation(
                    acquisition.thread_id(),
                    210,
                    211,
                    1,
                    0,
                ),
                resident_fixture::composer_support::fixture::operation_id(212),
            ))
        });
    }
    let creation_inputs = (count != 0).then(|| input.windows.clone());
    let retained = Rc::new(RefCell::new(None));
    let returned = retained.clone();
    let initial = Rc::new(RefCell::new(Vec::new()));
    let original = initial.clone();
    let captured = Rc::new(RefCell::new(Vec::new()));
    let placements = captured.clone();
    let natives = Rc::new(RefCell::new(Vec::new()));
    let handles = natives.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                move |result, app| {
                    eprintln!("mounted fixture startup returned");
                    let StartupCompletion::Running(process) = result else {
                        panic!("mounted fixture startup failed")
                    };
                    if let Some(inputs) = creation_inputs {
                        let creation = crate::main_window::MainWindowCreationOwner::install(
                            process.services.window_services(inputs).unwrap().creation_services(),
                            process.appearance.clone(),
                            crate::main_window::MainWindowCreationGate::Ready,
                            app,
                        )
                        .unwrap();
                        for shell in process.windows.shells() {
                            shell.attach_creation(creation.clone(), app);
                        }
                    }
                    let owner = RunningProcessOwner::start(process, app);
                    eprintln!("mounted fixture command consumer installed");
                    *original.borrow_mut() = snapshot(&owner).windows().to_vec();
                    *returned.borrow_mut() = Some(owner.clone());
                    app.spawn(async move |cx| {
                        for window in windows(&owner) {
                            eprintln!("mounted fixture observing native window");
                            handles.borrow_mut().push(native(window, cx).await);
                            eprintln!("mounted fixture capturing placement");
                            placements.borrow_mut().push(capture(window, cx).await);
                        }
                        eprintln!("mounted fixture entering scenario");
                        let diagnostic_owner = owner.clone();
                        scenario(owner, faults, cx).await;
                        eprintln!("mounted fixture scenario submitted final command");
                        for _ in 0..3 {
                            cx.background_executor()
                                .timer(Duration::from_millis(250))
                                .await;
                            eprintln!(
                                "mounted fixture after delivery: {:?}, shutdown: {:?}",
                                diagnostic_owner.borrow().test_ordinary_command_status(),
                                diagnostic_owner.borrow().shutdown_status()
                            );
                            cx.update(|app| {
                                for window in windows(&diagnostic_owner) {
                                    if let Ok(root) = window.read(app) {
                                        eprintln!(
                                            "mounted fixture command enabled: {}, presentation: {:?}",
                                            root.test_exit_command_enabled(),
                                            root.test_exit_presentation()
                                        );
                                    }
                                }
                            })
                            .unwrap();
                        }
                    })
                    .detach();
                },
                app,
            );
            support::watchdog_with_timeout(app, Duration::from_secs(30));
        });
    let owner = retained
        .borrow_mut()
        .take()
        .expect("running mount returned");
    if expect_shutdown {
        assert_eq!(owner.borrow().test_process().windows.shells().len(), 0);
        assert!(owner.borrow().test_services().graph().is_none());
        assert!(owner.borrow().test_final_teardown_detail().is_none());
        for native in natives.borrow().iter() {
            assert!(!unsafe { IsWindow(Some(*native)).as_bool() });
        }
    }
    drop(owner);
    let Some(restored_count) = restored_count else {
        let retained_path = directory.keep();
        if let Some(ledger) = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER") {
            std::fs::write(ledger, retained_path.to_string_lossy().as_bytes()).unwrap();
        }
        eprintln!(
            "retained ordinary command fixture awaiting process cleanup: {}",
            retained_path.display()
        );
        return None;
    };
    let StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    } = support::open(directory.path())
    else {
        panic!("reopen mounted fixture")
    };
    let home = candidate.publish().unwrap();
    let restored = state.session().minimal_bootstrap(&home).unwrap().unwrap();
    assert_eq!(restored.windows().len(), restored_count);
    for record in restored.windows() {
        let before = initial.borrow();
        if let Some(original) = before
            .iter()
            .find(|before| before.window_id() == record.window_id())
        {
            assert_eq!(
                record.selected_thread().map(|claim| claim.thread_id()),
                original.selected_thread().map(|claim| claim.thread_id())
            );
        }
        if let Some((_, placement)) = captured
            .borrow()
            .iter()
            .find(|(id, _)| *id == record.window_id())
        {
            assert_eq!(record.placement(), placement);
        }
    }
    drop((state, syndic));
    home.close().unwrap();
    Some(restored)
}

pub(super) async fn capture(
    window: Window,
    cx: &mut AsyncApp,
) -> (WindowId, beryl_model::WindowPlacement) {
    let (id, geometry, lease, released) = window
        .update(cx, |root, window, _| {
            let geometry = window.capture_windows_window_placement().unwrap();
            let (lease, released) = window.lease_published_windows_window().unwrap();
            (
                root.controller().unwrap().window_id(),
                geometry,
                lease,
                released,
            )
        })
        .unwrap();
    eprintln!("mounted fixture placement lease admitted");
    let desktop = cx
        .background_executor()
        .spawn(async move { crate::main_window::observe_windows_desktop(lease).unwrap() })
        .await;
    eprintln!("mounted fixture placement desktop observed");
    assert!(!released.await.unwrap().native_destroyed);
    eprintln!("mounted fixture placement lease released");
    (
        id,
        crate::main_window::windows_window_placement_from_capture(geometry, Some(desktop)).unwrap(),
    )
}

pub(super) fn windows(owner: &Owner) -> Vec<Window> {
    owner
        .borrow()
        .test_process()
        .windows
        .shells()
        .iter()
        .map(|shell| shell.window())
        .collect()
}

pub(super) fn snapshot(owner: &Owner) -> MinimalSessionBootstrap {
    let owner = owner.borrow();
    let graph = owner.test_services().graph().unwrap();
    graph
        .state()
        .session()
        .minimal_bootstrap(graph.home())
        .unwrap()
        .unwrap()
}

pub(super) fn retain_unviewed_work(
    owner: &Owner,
    thread: beryl_model::SyndicThreadId,
) -> impl Send + use<> {
    let owner = owner.borrow();
    let graph = owner.test_services().graph().unwrap();
    let storage = graph.syndic();
    let home = graph.home();
    let selected = graph
        .state()
        .session()
        .minimal_bootstrap(home)
        .unwrap()
        .unwrap();
    let execution = storage
        .thread_execution(
            home,
            selected.windows()[0].selected_thread().unwrap().thread_id(),
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let mut draft = *thread.as_bytes();
    draft[0] ^= 0x80;
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.create_thread(
            storage.revision(home).unwrap(),
            syndic_storage::CreateThread::ordinary(
                thread,
                beryl_model::SyndicDraftId::from_bytes(draft),
                execution,
                syndic_storage::SyndicTimestamp::from_unix_millis(1000),
                syndic_storage::DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    assert!(
        selected.windows().iter().all(|window| {
            window.selected_thread().map(|claim| claim.thread_id()) != Some(thread)
        })
    );
    crate::cas_projection::test_faults::retain_admitted_projection_work(graph.cas(), thread)
}

pub(super) async fn native(window: Window, cx: &mut AsyncApp) -> HWND {
    static TITLE: AtomicUsize = AtomicUsize::new(0);
    let title = format!(
        "ordinary-command-{}-{}",
        std::process::id(),
        TITLE.fetch_add(1, Ordering::SeqCst)
    );
    window
        .update(cx, |_, window, _| window.set_window_title(&title))
        .unwrap();
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.unwrap();
    assert!(unsafe { IsWindow(Some(handle)).as_bool() });
    handle
}

pub(super) fn post_close(window: HWND) {
    unsafe {
        PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
    }
}

pub(super) fn activate_exit(window: Window, cx: &mut AsyncApp) {
    window
        .update(cx, |root, _, _| root.test_activate_exit_command())
        .unwrap();
}

pub(super) async fn wait_until(cx: &mut AsyncApp, ready: impl Fn() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        if Instant::now() >= deadline {
            let diagnostic = cx
                .update(|app| {
                    let owner = RunningProcessOwner::mounted_owner(app)?.upgrade()?;
                    let status = owner.borrow().test_ordinary_command_status();
                    let shutdown = owner.borrow().shutdown_status();
                    let session = format!("{:?}", owner.borrow().shutdown_session());
                    let recovery = owner.borrow().automatic_recovery_outcome().map(|outcome| {
                        use crate::running_owner::InterruptedExitRecoveryOutcome;
                        match &*outcome {
                            InterruptedExitRecoveryOutcome::Running => "Running".to_owned(),
                            InterruptedExitRecoveryOutcome::Completed => "Completed".to_owned(),
                            InterruptedExitRecoveryOutcome::Cancelled => "Cancelled".to_owned(),
                            InterruptedExitRecoveryOutcome::Unavailable(error) => {
                                format!("Unavailable({error})")
                            }
                        }
                    });
                    let commands = windows(&owner)
                        .into_iter()
                        .filter_map(|window| {
                            window.read(app).ok().map(|root| {
                                (
                                    root.test_exit_command_enabled(),
                                    root.test_exit_presentation(),
                                )
                            })
                        })
                        .collect::<Vec<_>>();
                    Some((status, shutdown, session, recovery, commands))
                })
                .unwrap();
            panic!("{what} did not settle: {diagnostic:?}");
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) async fn wait_for_created_window(
    owner: &Owner,
    source: Window,
    count: usize,
    cx: &mut AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while owner.borrow().test_ordinary_command_status().0 != count {
        let status = cx
            .update(|app| source.read(app).unwrap().test_creation_owner_status(app))
            .unwrap()
            .expect("creation owner remains installed");
        assert!(status.1.is_none(), "window creation failed: {status:?}");
        assert!(
            Instant::now() < deadline,
            "created window publication did not settle: {status:?}"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) async fn wait_for_created_close(
    owner: &Owner,
    closing: Window,
    count: usize,
    cx: &mut AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while owner.borrow().test_ordinary_command_status().0 != count
        || owner.borrow().exit_requested()
    {
        if Instant::now() >= deadline {
            let status = owner.borrow().test_ordinary_command_status();
            let shutdown = owner.borrow().shutdown_status();
            let command = cx
                .update(|app| {
                    closing.read(app).ok().map(|root| {
                        (
                            root.test_exit_command_enabled(),
                            root.test_exit_presentation(),
                        )
                    })
                })
                .unwrap();
            panic!(
                "created window close did not settle: owner={status:?}, shutdown={shutdown:?}, command={command:?}"
            );
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) async fn composer(
    window: Window,
    cx: &mut AsyncApp,
) -> gpui::Entity<MainWindowConversationComposer> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(composer) = cx
            .update(|app| {
                window
                    .read(app)
                    .unwrap()
                    .controller()
                    .unwrap()
                    .composer_mount()
                    .unwrap()
                    .read(app)
                    .contribution()
            })
            .unwrap()
        {
            let ready = cx
                .update(|app| {
                    let input = composer.read(app).gpui_input().read(app);
                    input.surface().is_some() && input.is_quiescent()
                })
                .unwrap();
            if ready {
                return composer;
            }
        }
        assert!(Instant::now() < deadline, "mounted composer not ready");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) async fn dialog(cx: &mut AsyncApp) -> HWND {
    let title: Vec<u16> = "Exit Beryl?".encode_utf16().chain(Some(0)).collect();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(dialog) = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) } {
            return dialog;
        }
        assert!(
            Instant::now() < deadline,
            "mounted native confirmation not delivered"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) fn choose(dialog: HWND, button: i32) {
    unsafe {
        PostMessageW(
            Some(dialog),
            TDM_CLICK_BUTTON.0 as u32,
            WPARAM(button as usize),
            LPARAM(0),
        )
        .unwrap();
    }
}

pub(super) fn draft_text(chunks: usize) -> String {
    let mut chunk = vec![b'x'; 768];
    for offset in (63..768).step_by(64) {
        chunk[offset] = b'\n';
    }
    String::from_utf8(chunk).unwrap().repeat(chunks)
}

pub(super) fn capture_resident_history(
    owner: &Owner,
    thread: beryl_model::SyndicThreadId,
    binding: crate::composer_host::ComposerHostBinding,
) -> syndic_storage::DraftEditHistoryFrontierV1 {
    let owner = owner.borrow();
    let graph = owner.test_services().graph().unwrap();
    let current = graph
        .syndic()
        .current_draft_piece_text_demand(
            graph.home(),
            thread,
            syndic_storage::DraftPieceTextDemandV1::Forward(0),
            4096,
        )
        .unwrap()
        .unwrap();
    let saved_operation = match binding.history().key() {
        syndic_storage::DraftEditHistoryFrontierKeyV1::Publication { operation_id, .. } => {
            Some(operation_id)
        }
        _ => None,
    };
    let source = graph
        .syndic()
        .capture_draft_editor_candidate_publication_source(
            graph.home(),
            syndic_storage::DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                current.selector(),
                binding.candidate(),
                saved_operation.unwrap_or_else(|| {
                    syndic_storage::DraftPieceOperationIdV1::from_bytes([248; 16])
                }),
                SyndicTimestamp::from_unix_millis(4),
            ),
        )
        .unwrap();
    let prepared = graph
        .syndic()
        .prepare_draft_editor_candidate_publication(
            graph.home(),
            source,
            syndic_storage::DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap();
    if saved_operation.is_some() {
        assert_eq!(prepared.captured_frontier().reference(), binding.history());
    }
    prepared.captured_frontier().clone()
}

pub(super) async fn copy_all(
    window: Window,
    resident: &gpui::Entity<MainWindowConversationComposer>,
    cx: &mut AsyncApp,
) -> String {
    let copied = Rc::new(RefCell::new(None));
    let written = copied.clone();
    window
        .update(cx, |_, window, app| {
            resident.update(app, |composer, _| {
                composer.test_set_clipboard_writer(Box::new(move |text, _| {
                    *written.borrow_mut() = Some(text.to_owned());
                    gpui_text_input::ClipboardWriteOutcome::Written
                }))
            });
            resident
                .read(app)
                .gpui_input()
                .update(app, |input, _| input.focus(window));
            window.dispatch_action(Box::new(gpui_text_input::SelectAll), app);
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let ready = cx
            .update(|app| resident.read(app).gpui_input().read(app).is_quiescent())
            .unwrap();
        if ready {
            break;
        }
        assert!(Instant::now() < deadline, "selection did not settle");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    window
        .update(cx, |_, window, app| {
            window.dispatch_action(Box::new(gpui_text_input::Copy), app)
        })
        .unwrap();
    wait_until(cx, || copied.borrow().is_some(), "resident copy").await;
    copied.borrow_mut().take().unwrap()
}
