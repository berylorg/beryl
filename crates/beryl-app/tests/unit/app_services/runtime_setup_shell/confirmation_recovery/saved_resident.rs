use super::*;

pub(super) fn seed_prepublication_prior(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    faults: FaultController,
    cx: &mut TestAppContext,
) {
    let native_window = window.window_id();
    let original = selection(window, cx);
    let original_history = capture_history(owner, original);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let home = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .home()
        .service_reference();
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_hooks(
                None,
                Some(Box::new(move |_| {
                    faults.fail_next(FaultPoint::BeforeReadConfirmation);
                    assert!(home.home_revision().is_err());
                    assert_eq!(
                        home.health().state(),
                        beryl_home_store::HomeHealthState::Failed
                    );
                })),
                None,
            );
        })
        .unwrap();
    confirm(owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    assert!(recover_creation(owner, window, cx) > 0);
    let restored = selection(window, cx);
    assert_eq!(window.window_id(), native_window);
    assert_eq!(restored.window_id(), original.window_id());
    assert_eq!(restored.claim(), original.claim());
    assert_eq!(
        restored.binding().candidate().draft_id(),
        original.binding().candidate().draft_id()
    );
    assert_eq!(restored.binding().root(), original.binding().root());
    assert_eq!(
        restored.binding().logical_extent(),
        original.binding().logical_extent()
    );
    assert_ne!(
        restored.binding().home_generation(),
        original.binding().home_generation()
    );
    assert_preserved_history(&original_history, &capture_history(owner, restored));
    assert!(
        window
            .read_with(cx, |root, app| root
                .test_thread_creation_composer_adopted_custody_items(app))
            .unwrap()
            > 0
    );
    {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        let text = graph
            .syndic()
            .current_draft_piece_text_demand(
                graph.home(),
                restored.claim().thread_id(),
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                4096,
            )
            .unwrap()
            .unwrap();
        assert_eq!(text.value().bytes(), b"saved before failed creation");
    }
    let (services, reader, member) = {
        let published = owner.borrow();
        let graph = published.test_services().graph().unwrap();
        let member = graph
            .state()
            .session()
            .capture_window_removal(graph.home(), restored.window_id())
            .unwrap();
        assert_eq!(member.window().selected_thread(), Some(restored.claim()));
        (
            published.test_services().runtime_setup_services().unwrap(),
            published.test_services().running_threads_reader().unwrap(),
            member.window().window_id(),
        )
    };
    window
        .update(cx, |root, window, cx| {
            root.test_runtime_setup_fixture(services, vec![member], Some(reader), window, cx);
            let diagnostics = root.test_thread_creation_picker_open_diagnostics();
            assert!(diagnostics.contains("enabled=true"), "{diagnostics}");
            assert!(
                diagnostics.contains("services_current=true"),
                "{diagnostics}"
            );
        })
        .unwrap();
}

pub(super) fn reopen(
    directory: &tempfile::TempDir,
    expected: MainWindowComposerSelectionIdentity,
    original_history: syndic_storage::DraftEditHistoryFrontierV1,
    cx: &mut TestAppContext,
) -> (
    Rc<RefCell<RunningProcessOwner>>,
    gpui::WindowHandle<MainWindowShellRoot>,
    FaultController,
) {
    let path = directory.path().to_owned();
    let (mut process, prepared, appearance, faults) = std::thread::spawn(move || {
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let candidate = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        let mut process = owner(&candidate);
        process
            .open_initial(
                candidate,
                state,
                storage,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                CommandCancellation::new(),
            )
            .unwrap();
        process
            .graph_mut()
            .unwrap()
            .catalog_source
            .as_mut()
            .unwrap()
            .stop_and_join()
            .unwrap();
        process.graph_mut().unwrap().release_theme().unwrap();
        let appearance = process
            .graph_mut()
            .unwrap()
            .theme()
            .unwrap()
            .current()
            .unwrap();
        let mut inputs = window_services::inputs();
        inputs.configurator_source = Arc::new(|| Box::new(configure));
        inputs.restored_activation_source = Arc::new(move |record| {
            assert_eq!(record.window_id(), expected.window_id());
            assert_eq!(
                record.selected_thread().unwrap().thread_id(),
                expected.claim().thread_id()
            );
            Ok((
                widget_support::activation(
                    expected.claim().thread_id(),
                    211,
                    212,
                    1,
                    expected.binding().logical_extent().logical_utf8_bytes(),
                ),
                widget_support::fixture::operation_id(213),
            ))
        });
        let mut work = process
            .window_services(inputs)
            .unwrap()
            .into_restore_set(
                appearance.clone(),
                expected.window_id(),
                window_services::placement(),
            )
            .unwrap();
        for _ in 0..64 {
            match work.advance() {
                MainWindowRestoreSetOutcome::Pending(next) => work = next,
                MainWindowRestoreSetOutcome::Prepared(prepared) => {
                    return (process, prepared, appearance, faults);
                }
                MainWindowRestoreSetOutcome::Retained { reason, .. } => {
                    panic!("saved resident restore retained: {reason:?}")
                }
                MainWindowRestoreSetOutcome::Failed { error } => {
                    panic!("saved resident restore failed: {error}")
                }
            }
        }
        panic!("saved resident restore did not settle")
    })
    .join()
    .unwrap();
    let services = process.runtime_setup_services().unwrap();
    let reader = process.running_threads_reader().unwrap();
    let appearance = cx
        .update(|app| GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app));
    let mut fixture = cx.update(|app| {
        PublishedMainWindowRestoreSet::prepare_virtual_restored_test(
            prepared,
            appearance.clone(),
            app,
        )
        .unwrap()
    });
    let mut published = None;
    wait(
        cx,
        |cx| {
            published = cx.update(|app| fixture.advance(app).unwrap());
            published.is_some()
        },
        "saved resident did not reach normal hidden publication readiness",
    );
    let windows = published.unwrap();
    let (owner, window) = cx.update(|app| {
        assert_eq!(windows.shells().len(), 1);
        let window = windows.shells()[0].window();
        let owner = RunningProcessOwner::test_start_unmounted(
            StartedProcess {
                configuration: configuration(),
                services: process,
                windows,
                appearance,
                startup_surface: None,
                commands: StartupCommands::test_running(),
            },
            app,
        );
        window
            .update(app, |root, window, cx| {
                root.test_runtime_setup_fixture(
                    services,
                    vec![expected.window_id()],
                    Some(reader),
                    window,
                    cx,
                );
            })
            .unwrap();
        (owner, window)
    });
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, app| {
                    root.controller()
                        .unwrap()
                        .composer_mount()
                        .is_some_and(|mount| mount.read(app).contribution().is_some())
                })
                .unwrap()
        },
        "saved resident did not mount",
    );
    let restored = selection(window, cx);
    assert_eq!(restored.window_id(), expected.window_id());
    assert_eq!(restored.claim().thread_id(), expected.claim().thread_id());
    assert_eq!(
        restored.binding().candidate().draft_id(),
        expected.binding().candidate().draft_id()
    );
    assert_ne!(
        restored.binding().candidate().session_id(),
        expected.binding().candidate().session_id()
    );
    assert_eq!(
        restored.binding().logical_extent(),
        expected.binding().logical_extent()
    );
    assert_preserved_history(&original_history, &capture_history(&owner, restored));
    assert_canonical_publication(&owner, restored, original_history.reference());
    assert_eq!(
        restored.binding().history().key(),
        syndic_storage::DraftEditHistoryFrontierKeyV1::session(
            expected.binding().candidate().draft_id(),
            restored.binding().candidate().session_id(),
        )
    );
    (owner, window, faults)
}

pub(super) fn capture_history(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    selection: MainWindowComposerSelectionIdentity,
) -> syndic_storage::DraftEditHistoryFrontierV1 {
    let retained = owner.borrow();
    let graph = retained.test_services().graph().unwrap();
    let current = graph
        .syndic()
        .current_draft_piece_text_demand(
            graph.home(),
            selection.claim().thread_id(),
            syndic_storage::DraftPieceTextDemandV1::Forward(0),
            4096,
        )
        .unwrap()
        .unwrap();
    let binding = selection.binding();
    let saved_operation = binding.history().key().publication_operation_id();
    let operation = saved_operation
        .unwrap_or_else(|| syndic_storage::DraftPieceOperationIdV1::from_bytes([248; 16]));
    let source = graph
        .syndic()
        .capture_draft_editor_candidate_publication_source(
            graph.home(),
            syndic_storage::DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                current.selector(),
                binding.candidate(),
                operation,
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
    let request = prepared.request();
    assert_eq!(request.selector(), current.selector());
    assert_eq!(request.session_id(), binding.candidate().session_id());
    assert_eq!(
        request.candidate_generation(),
        binding.candidate().candidate_generation()
    );
    assert_eq!(request.candidate().root(), binding.candidate().root());
    assert_eq!(request.candidate().history(), binding.history());
    let captured = prepared.captured_frontier().reference();
    assert_eq!(
        captured.key(),
        syndic_storage::DraftEditHistoryFrontierKeyV1::publication(
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
            operation,
        )
    );
    assert_eq!(captured.root(), binding.history().root());
    assert_eq!(
        captured.candidate_generation(),
        binding.history().candidate_generation()
    );
    assert_eq!(
        captured.frontier_revision(),
        binding.history().frontier_revision()
    );
    assert_eq!(captured.byte_budget(), binding.history().byte_budget());
    assert_eq!(
        captured.retention_policy_revision(),
        binding.history().retention_policy_revision()
    );
    assert_eq!(captured.availability(), binding.history().availability());
    if captured.key() == binding.history().key() {
        assert_eq!(captured, binding.history());
    }
    prepared.captured_frontier().clone()
}

pub(super) fn assert_canonical_publication(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    selected: MainWindowComposerSelectionIdentity,
    original: syndic_storage::DraftEditHistoryFrontierReferenceV1,
) {
    let retained = owner.borrow();
    let graph = retained.test_services().graph().unwrap();
    let current = graph
        .syndic()
        .current_draft_piece_text_demand(
            graph.home(),
            selected.claim().thread_id(),
            syndic_storage::DraftPieceTextDemandV1::Forward(0),
            4096,
        )
        .unwrap()
        .unwrap();
    let history = current.selector().history();
    assert_eq!(history.key().draft_id(), original.key().draft_id());
    assert_eq!(history.key().session_id(), original.key().session_id());
    assert!(history.key().publication_operation_id().is_some());
    assert_eq!(history.root(), original.root());
    assert_eq!(
        history.candidate_generation(),
        original.candidate_generation()
    );
    assert_eq!(history.frontier_revision(), original.frontier_revision());
}

pub(super) fn assert_preserved_history(
    original: &syndic_storage::DraftEditHistoryFrontierV1,
    current: &syndic_storage::DraftEditHistoryFrontierV1,
) {
    let before = original.reference();
    let after = current.reference();
    assert_eq!(after.key().draft_id(), before.key().draft_id());
    assert_eq!(after.root(), before.root());
    assert_eq!(after.candidate_generation(), before.candidate_generation());
    assert_eq!(after.frontier_revision(), before.frontier_revision());
    assert_eq!(after.byte_budget(), before.byte_budget());
    assert_eq!(
        after.retention_policy_revision(),
        before.retention_policy_revision()
    );
    assert_eq!(after.availability(), before.availability());
    assert_eq!(current.journal_head(), original.journal_head());
    assert_eq!(current.undo_head(), original.undo_head());
    assert_eq!(current.redo_head(), original.redo_head());
    assert_eq!(current.oldest_eligible(), original.oldest_eligible());
    assert_eq!(
        current.cumulative_encoded_bytes(),
        original.cumulative_encoded_bytes()
    );
    assert_eq!(
        current.retained_encoded_bytes(),
        original.retained_encoded_bytes()
    );
}
