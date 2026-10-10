use super::*;

#[derive(Clone, Copy)]
pub(super) enum Entry {
    Prepared,
    PreparedWithParentFocus,
    Switcher,
    Navigation,
    Lineage(lineage_entry::Input),
}

pub(super) async fn recover(
    owner: Rc<RefCell<RunningProcessOwner>>,
    faults: FaultController,
    cut: Cut,
    control: control::Control,
    save: SaveExpectation,
    cx: &mut gpui::AsyncApp,
) -> (
    WindowHandle<MainWindowShellRoot>,
    beryl_model::WindowId,
    beryl_state::WindowClaimSelection,
) {
    recover_with_entry(owner, faults, cut, control, save, Entry::Prepared, cx).await
}

pub(super) async fn recover_with_entry(
    owner: Rc<RefCell<RunningProcessOwner>>,
    faults: FaultController,
    cut: Cut,
    control: control::Control,
    save: SaveExpectation,
    entry: Entry,
    cx: &mut gpui::AsyncApp,
) -> (
    WindowHandle<MainWindowShellRoot>,
    beryl_model::WindowId,
    beryl_state::WindowClaimSelection,
) {
    let original_windows = windows(&owner);
    let invoking = original_windows[0];
    if matches!(entry, Entry::Navigation) {
        navigation_entry::prepare(invoking, cx).await;
    }
    if matches!(entry, Entry::Lineage(_)) {
        lineage_entry::prepare(invoking, cx).await;
    }
    let unrelated_window = original_windows[1];
    let dirty = !matches!(save, SaveExpectation::SavedNoop);
    let mut prior = if matches!(save, SaveExpectation::DirtyPageSetup) {
        let height = measure_pending_target_page_view(&owner, invoking, cx).await;
        preservation::capture_original_page_resident(&owner, invoking, 0, height, cx).await
    } else {
        preservation::capture_resident(&owner, invoking, 0, dirty, cx).await
    };
    let unrelated = preservation::capture_resident(&owner, unrelated_window, 1, dirty, cx).await;
    let mut marker = if matches!(save, SaveExpectation::DirtyMarker) {
        assert!(matches!(cut, Cut::DisposalCommitted));
        Some(marker_origin::seed(&owner, invoking, cx).await)
    } else {
        None
    };
    let original_editor = composer(invoking, cx).await;
    let original_identity = cx
        .update(|app| original_editor.read(app).selection_identity())
        .unwrap();
    let original_input = cx
        .update(|app| original_editor.read(app).gpui_input())
        .unwrap();
    let publication = crate::composer_host::ComposerHostPublicationExecutionObservation::new();
    let disposal = crate::composer_host::ComposerHostDisposalExecutionObservation::new();
    invoking
        .update(cx, |root, _, app| {
            root.test_observe_selected_publication_execution(
                original_identity,
                publication.clone(),
                app,
            )
            .unwrap();
            root.test_observe_selected_disposal_execution(original_identity, disposal.clone(), app)
                .unwrap();
        })
        .unwrap();
    let original_outcome = outcome_evidence::ClaimOutcomeObservation::default();
    let observed_outcome = original_outcome.clone();
    let original = snapshot(&owner);
    let invoking_id = cx
        .update(|app| {
            invoking
                .read(app)
                .unwrap()
                .controller()
                .unwrap()
                .window_id()
        })
        .unwrap();
    let original_record = original
        .windows()
        .iter()
        .find(|record| record.window_id() == invoking_id)
        .unwrap()
        .clone();
    let original_claim = original_record.selected_thread().unwrap();
    let unrelated_record = original
        .windows()
        .iter()
        .find(|record| record.window_id() != original_record.window_id())
        .unwrap()
        .clone();
    let native_handles = vec![
        native(invoking, cx).await,
        native(unrelated_window, cx).await,
    ];
    let placements = vec![
        capture(invoking, cx).await,
        capture(unrelated_window, cx).await,
    ];
    wait_for_unviewed_catalog_source(&owner, original_claim.thread_id(), cx).await;
    let target = SyndicThreadId::from_bytes([242; 16]);
    let target_draft = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        graph
            .syndic()
            .current_draft(
                graph.home(),
                target,
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap()
            .draft()
            .id()
    };
    let (home, state, storage) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (
            graph.home().service_reference(),
            graph.state().clone(),
            graph.syndic().clone(),
        )
    };
    wait_for_unviewed_catalog_source(&owner, target, cx).await;
    let prepare_home = home.clone();
    let (prepared, duplicate) = cx
        .background_executor()
        .spawn(async move {
            let prepare = || {
                let prepared = RunningThreadActivation::prepare(
                    &prepare_home,
                    &state,
                    &storage,
                    original_record.window_id(),
                    Some(original_claim),
                    original_record.remembered_target().unwrap(),
                    target,
                )
                .unwrap();
                let RunningThreadActivationPreparation::Prepared(prepared) = prepared else {
                    panic!("unviewed ordinary target was not prepared");
                };
                Box::new(prepared)
            };
            (prepare(), prepare())
        })
        .await;
    let expected_window = prepared.future_window().clone();
    let expected_claim = prepared.future_selection();
    let capture_home = home.clone();
    let capture_storage = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .syndic()
        .clone();
    let durable = cx
        .background_executor()
        .spawn(async move {
            durable_evidence::OriginalDurableEvidence::capture(
                &capture_home,
                &capture_storage,
                original_identity,
            )
        })
        .await;
    let picker = match entry {
        Entry::Prepared => None,
        Entry::PreparedWithParentFocus => {
            lineage_entry::ready(invoking, target, cx).await;
            lineage_entry::focus(invoking, target, cx).await;
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let settled = invoking
                    .update(cx, |root, window, app| {
                        let resident = original_editor.read(app);
                        let input = original_input.read(app);
                        input.is_quiescent()
                            && input.is_surface_current_and_interactive()
                            && !resident.test_has_active_flight()
                            && !resident.test_has_pending_realizer()
                            && root.test_thread_lineage_focus(
                                original_claim.thread_id(),
                                target,
                                window,
                                app,
                            )
                    })
                    .unwrap();
                if settled {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "original parent-lineage focus and Page geometry did not settle"
                );
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
            }
            prior.expect_restored_lineage_focus(original_claim.thread_id(), target);
            None
        }
        Entry::Switcher => {
            let picker = switcher_entry::open(invoking, target, cx).await;
            let owner_focus = invoking
                .read_with(cx, |root, _| root.test_thread_switcher_focus())
                .unwrap();
            prior.expect_restored_focus(owner_focus);
            Some(picker)
        }
        Entry::Navigation => {
            navigation_entry::ready(invoking, true, cx).await;
            let focus = invoking
                .read_with(cx, |root, _| root.test_thread_navigation_focus(true))
                .unwrap();
            invoking
                .update(cx, |_, window, _| focus.focus(window))
                .unwrap();
            prior.expect_restored_focus(focus);
            None
        }
        Entry::Lineage(_) => {
            lineage_entry::ready(invoking, target, cx).await;
            lineage_entry::focus(invoking, target, cx).await;
            prior.expect_restored_lineage_focus(original_claim.thread_id(), target);
            None
        }
    };
    if matches!(save, SaveExpectation::DirtyPageSetup) {
        prior.authenticate_original_page_custody(cx);
    }
    let failed_generation = home.health().generation().unwrap();
    control::install(&owner, invoking, control, cx).await;
    let reached = Arc::new(AtomicBool::new(false));
    let failed_indeterminate = Arc::new(AtomicBool::new(false));
    let hook_reached = reached.clone();
    let hook_faults = faults.clone();
    let scope = match cut {
        Cut::SaveNoncommit | Cut::SaveIndeterminate => {
            syndic_storage::test_faults::draft_candidate_publication_fault_scope()
        }
        Cut::DisposalCommitted => {
            syndic_storage::test_faults::draft_candidate_session_disposal_fault_scope()
        }
        _ => FaultScope::of::<beryl_state::ReplaceWindowClaim>(),
    };
    let point = match cut {
        Cut::SaveNoncommit | Cut::ClaimNoncommit => FaultPoint::BeforeCommit,
        Cut::SaveIndeterminate | Cut::ClaimIndeterminate => FaultPoint::AfterCommitBeforePersist,
        Cut::ClaimCommitted | Cut::DisposalCommitted => FaultPoint::AfterPersist,
    };
    let hook: Box<dyn FnOnce(&CommandCancellation) + Send> =
        Box::new(move |_: &CommandCancellation| {
            hook_faults.fail_next_in_scope(point, scope);
            hook_reached.store(true, Ordering::Release);
        });
    let fail_faults = faults.clone();
    let fail_reached = failed_indeterminate.clone();
    let claim_failure_home = home.clone();
    invoking
                    .update(cx, |root, window, app| {
                        let (commit, save, disposal) = match cut {
                            Cut::SaveNoncommit | Cut::SaveIndeterminate => (None, Some(hook), None),
                            Cut::DisposalCommitted => (None, None, Some(hook)),
                            _ => (Some(hook), None, None),
                        };
                        root.test_thread_confirmation_hooks(commit, save, disposal);
                        if matches!(cut, Cut::SaveIndeterminate) {
                            let fail_faults = fail_faults.clone();
                            let fail_reached = fail_reached.clone();
                            root.test_after_selected_publication_execute(
                                original_identity,
                                Box::new(move |home, outcome| {
                                    assert!(matches!(outcome, CommandOutcome::Indeterminate { .. }));
                                    outcome_evidence::fail_original_confirmed_read(home, &fail_faults, &fail_reached);
                                }),
                                app,
                            ).unwrap();
                        }
                        root.test_original_ordinary_claim_outcome_hook(Box::new(move |outcome| {
                            observed_outcome.record(outcome);
                            if matches!(cut, Cut::ClaimIndeterminate) {
                                let crate::main_window::running_threads::activation::RunningThreadActivationOutcome::Pending(pending) = outcome else {
                                    panic!("original claim must retain an indeterminate outcome");
                                };
                                assert!(pending.test_original_commit_receipt().is_none());
                                outcome_evidence::fail_original_confirmed_read(&claim_failure_home, &fail_faults, &fail_reached);
                            }
                        }));
                        if let Entry::Lineage(input) = entry {
                            drop((prepared, duplicate));
                            lineage_entry::post_input(root, target, input, native_handles[0], window, app);
                        } else if matches!(entry, Entry::Navigation) {
                            drop((prepared, duplicate));
                            navigation_entry::post_space(native_handles[0]);
                        } else if let Some((picker, key)) = picker {
                            drop((prepared, duplicate));
                            picker.update(app, |picker, pcx| picker.activate(&key, pcx));
                        } else {
                            root.test_begin_original_ordinary_selection(*prepared, window, app)
                                .unwrap();
                            assert_eq!(
                                root.test_begin_original_ordinary_selection(*duplicate, window, app)
                                    .unwrap_err(),
                                "original ordinary selection fixture window changed"
                            );
                        }
                    })
                    .unwrap();
    drop(home);
    control::complete(&owner, invoking, control, &reached, failed_generation, cx).await;
    if matches!(save, SaveExpectation::DirtyPageSetup) {
        let adopted = cx
            .update(|app| {
                invoking
                    .read(app)
                    .unwrap()
                    .test_thread_creation_composer_adopted_custody_items(app)
            })
            .unwrap();
        assert!(
            adopted > 0,
            "fresh prior recovery owns no adopted Page custody before verification"
        );
    }
    assert!(reached.load(Ordering::Acquire));
    if matches!(cut, Cut::SaveIndeterminate | Cut::ClaimIndeterminate) {
        assert!(failed_indeterminate.load(Ordering::Acquire));
    }
    original_outcome.verify(cut);
    let executed = publication.snapshot();
    assert_eq!(
        executed.original_attempts,
        usize::from(dirty),
        "{executed:?}"
    );
    assert_eq!(executed.different_identity_attempts, 0, "{executed:?}");
    let original_save = executed.original;
    let disposed = disposal.snapshot();
    assert_eq!(disposed.different_identity_attempts, 0, "{disposed:?}");
    if matches!(cut, Cut::DisposalCommitted) {
        assert_eq!(disposed.original_attempts, 1, "{disposed:?}");
        let identity = disposed.original.unwrap();
        assert_eq!(
            identity.draft_id,
            original_identity.binding().candidate().draft_id()
        );
        assert_eq!(
            identity.session_id,
            original_identity.binding().candidate().session_id()
        );
    } else {
        assert_eq!(disposed.original_attempts, 0, "{disposed:?}");
    }
    if let Some(original_save) = original_save {
        assert!(dirty);
        assert_eq!(
            original_save.draft_id,
            original_identity.binding().candidate().draft_id()
        );
        assert_eq!(
            original_save.session_id,
            original_identity.binding().candidate().session_id()
        );
    } else {
        assert!(!dirty);
    }
    let (verify_home, verify_storage) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (graph.home().service_reference(), graph.syndic().clone())
    };
    let durable = cx
        .background_executor()
        .spawn(async move {
            let mut durable = durable;
            match save {
                SaveExpectation::Dirty
                | SaveExpectation::DirtyMarker
                | SaveExpectation::DirtyPageSetup => durable.verify_settled(
                    &verify_home,
                    &verify_storage,
                    cut,
                    original_save.unwrap(),
                    disposed.original,
                ),
                SaveExpectation::SavedNoop => durable.verify_saved_noop(
                    &verify_home,
                    &verify_storage,
                    cut,
                    disposed.original.unwrap(),
                ),
            }
            durable
        })
        .await;
    assert_eq!(windows(&owner), original_windows);
    let fresh = snapshot(&owner);
    let selected = fresh
        .windows()
        .iter()
        .find(|record| record.window_id() == expected_window.window_id())
        .unwrap();
    assert_eq!(
        fresh
            .windows()
            .iter()
            .find(|record| record.window_id() == unrelated_record.window_id())
            .unwrap(),
        &unrelated_record
    );
    if cut.committed() {
        assert_eq!(selected, &expected_window);
        assert_eq!(selected.selected_thread(), Some(expected_claim));
        let resident = composer(invoking, cx).await;
        let binding = cx
            .update(|app| resident.read(app).selection_identity())
            .unwrap();
        assert_eq!(binding.claim(), expected_claim);
        assert_ne!(binding.binding().home_generation(), failed_generation);
        assert_eq!(binding.binding().logical_extent().logical_utf8_bytes(), 768);
        assert_eq!(binding.binding().candidate().draft_id(), target_draft);
        assert_eq!(copy_all(invoking, &resident, cx).await, draft_text(1));
        if matches!(entry, Entry::Lineage(_)) {
            transcript::assert_recovered_with_records(invoking, 3, cx).await;
        } else {
            transcript::assert_recovered(invoking, cx).await;
        }
        if matches!(entry, Entry::Lineage(_)) {
            invoking
                .update(cx, |root, window, app| {
                    assert!(root.test_thread_lineage_view().is_none());
                    assert!(root.notice_safe_focus(app).is_focused(window));
                })
                .unwrap();
        }
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        assert!(
            graph
                .state()
                .session()
                .thread_claim_catalog_source(graph.home(), original_claim.thread_id())
                .unwrap()
                .claim()
                .is_none()
        );
        let claim = graph
            .state()
            .session()
            .window_claim_catalog_source(graph.home(), selected.window_id())
            .unwrap()
            .claim()
            .unwrap();
        assert_eq!(claim.window_id(), selected.window_id());
        assert_eq!(claim.thread_id(), expected_claim.thread_id());
        assert_eq!(claim.generation(), expected_claim.generation());
        assert_eq!(claim.revision(), expected_claim.revision());
        assert_eq!(claim.state(), beryl_state::ThreadClaimState::Active);
        drop(retained);
    } else {
        assert_eq!(selected.selected_thread(), Some(original_claim));
        if matches!(save, SaveExpectation::DirtyPageSetup) {
            preservation::verify_resident_preserving_page_custody(&owner, prior, cx).await;
        } else {
            preservation::verify_resident(&owner, prior, cx).await;
        }
    }
    let settled_claim = selected.selected_thread().unwrap();
    let settled_window = selected.window_id();
    if matches!(save, SaveExpectation::DirtyPageSetup) {
        preservation::verify_resident_preserving_page_custody(&owner, unrelated, cx).await;
    } else {
        preservation::verify_resident(&owner, unrelated, cx).await;
    }
    assert!(!owner.borrow().test_services().running_selection_pending());
    assert!(!owner.borrow().exit_requested());
    assert!(owner.borrow().shutdown_status().is_none());
    let released = if dirty {
        None
    } else {
        Some(page_custody::accepted_original_release(invoking, original_identity, cx).await)
    };
    if let Some(marker) = &mut marker {
        marker.verify(&owner, cx).await;
    }
    control::reject_stale_generation(&owner, failed_generation, cx).await;
    if let Some(marker) = &mut marker {
        marker.verify(&owner, cx).await;
    }
    if let Some(released) = released {
        assert_eq!(page_custody::current_release(invoking, cx).await, released);
    }
    assert_eq!(disposal.snapshot(), disposed);
    assert_eq!(publication.snapshot(), executed);
    let (verify_home, verify_storage) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (graph.home().service_reference(), graph.syndic().clone())
    };
    cx.background_executor()
        .spawn(async move {
            durable.verify_unchanged(&verify_home, &verify_storage);
        })
        .await;
    for ((window, handle), placement) in original_windows
        .iter()
        .zip(&native_handles)
        .zip(&placements)
    {
        assert_eq!(native(*window, cx).await, *handle);
        assert_eq!(&capture(*window, cx).await, placement);
        assert!(unsafe { IsWindow(Some(*handle)).as_bool() });
    }
    native_support::wait_for_notice(&original_windows, true, cx).await;
    if matches!(entry, Entry::Switcher) {
        cx.update(|app| {
            let root = invoking.read(app).unwrap();
            assert!(root.test_thread_switcher_picker().is_none());
            let expected_history = if cut.committed() {
                vec![original_claim.thread_id(), target]
            } else {
                Vec::new()
            };
            assert_eq!(root.test_thread_navigation_history(), expected_history);
        })
        .unwrap();
    }
    if matches!(entry, Entry::Navigation) {
        cx.update(|app| {
            let root = invoking.read(app).unwrap();
            assert_eq!(
                root.test_thread_navigation_history(),
                vec![original_claim.thread_id(), target]
            );
        })
        .unwrap();
    }
    if matches!(entry, Entry::Lineage(_)) {
        lineage_entry::assert_history_settled_once(
            invoking,
            original_claim.thread_id(),
            target,
            cut.committed(),
            cx,
        )
        .await;
    }
    (invoking, settled_window, settled_claim)
}

async fn measure_pending_target_page_view(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::AsyncApp,
) -> gpui::Pixels {
    let resident = composer(window, cx).await;
    let original_identity = resident
        .read_with(cx, |resident, _| resident.selection_identity())
        .unwrap();
    let id = window
        .read_with(cx, |root, _| root.controller().unwrap().window_id())
        .unwrap();
    let record = snapshot(owner)
        .windows()
        .iter()
        .find(|record| record.window_id() == id)
        .unwrap()
        .clone();
    let target = SyndicThreadId::from_bytes([242; 16]);
    wait_for_unviewed_catalog_source(owner, target, cx).await;
    let (home, state, storage) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (
            graph.home().service_reference(),
            graph.state().clone(),
            graph.syndic().clone(),
        )
    };
    let prepared = cx
        .background_executor()
        .spawn(async move {
            let RunningThreadActivationPreparation::Prepared(prepared) =
                RunningThreadActivation::prepare(
                    &home,
                    &state,
                    &storage,
                    id,
                    record.selected_thread(),
                    record.remembered_target().unwrap(),
                    target,
                )
                .unwrap()
            else {
                panic!("Page admission target was not prepared");
            };
            Box::new(prepared)
        })
        .await;
    let observation = Arc::new(std::sync::Mutex::new(None));
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_acknowledgement = cancelled.clone();
    assert_original_producer_current(
        owner,
        original_identity.binding(),
        "before Page admission target activation",
    );
    window
        .update(cx, |root, window, app| {
            root.test_observe_ordinary_pending_ready(Some(observation.clone()));
            root.test_thread_confirmation_hooks(
                Some(Box::new(move |cancellation| {
                    cancellation.cancel();
                    cancel_acknowledgement.store(true, Ordering::Release);
                })),
                None,
                None,
            );
            root.test_begin_original_ordinary_selection(*prepared, window, app)
                .unwrap();
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let settled = cancelled.load(Ordering::Acquire)
            && !owner.borrow().test_services().running_selection_pending()
            && window
                .read_with(cx, |root, _| !root.test_running_thread_activation_pending())
                .unwrap();
        if settled {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Page admission target cancellation did not settle"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    let (observed_target, target_identity, height) = observation
        .lock()
        .unwrap()
        .clone()
        .expect("ordinary target supplied no staged Ready geometry");
    assert_eq!(observed_target, target);
    assert_eq!(target_identity.claim().thread_id(), target);
    assert!(height > gpui::Pixels::ZERO && f32::from(height).is_finite());
    window
        .update(cx, |root, _, _| {
            root.test_observe_ordinary_pending_ready(None)
        })
        .unwrap();
    let restored = composer(window, cx).await;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let settled = restored
            .read_with(cx, |resident, app| {
                assert_eq!(resident.selection_identity(), original_identity);
                let input = resident.gpui_input();
                let input = input.read(app);
                input.is_enabled()
                    && input.is_quiescent()
                    && input.is_surface_current_and_interactive()
                    && !resident.test_has_active_flight()
                    && !resident.test_has_pending_realizer()
            })
            .unwrap();
        if settled {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original editor did not settle after Page admission target cancellation"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert_eq!(
        snapshot(owner)
            .windows()
            .iter()
            .find(|record| record.window_id() == id)
            .unwrap()
            .selected_thread(),
        Some(original_identity.claim())
    );
    assert_original_producer_current(
        owner,
        original_identity.binding(),
        "after Page admission target cancellation",
    );
    height
}

fn assert_original_producer_current(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    binding: crate::composer_host::ComposerHostBinding,
    cut: &str,
) {
    let candidate = binding.candidate();
    let retained = owner.borrow();
    let graph = retained.test_services().graph().unwrap();
    let outcome = graph
        .syndic()
        .draft_editor_candidate_session(graph.home(), candidate.draft_id(), candidate.session_id())
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(head) = outcome else {
        panic!("original producer was not Active {cut}: expected={binding:?}, actual={outcome:?}");
    };
    assert!(
        head.draft_id() == candidate.draft_id()
            && head.session_id() == candidate.session_id()
            && head.session_generation() == candidate.session_generation(),
        "original producer generation changed {cut}: expected={binding:?}, actual={head:?}"
    );
}
