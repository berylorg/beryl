use super::*;
use crate::running_owner::InterruptedExitRecoveryOutcome;
use confirmation::{confirm, edit_prior, selected_window, selection};
use std::sync::Mutex;

#[path = "confirmation_recovery/failure_gates.rs"]
mod failure_gates;
#[path = "confirmation_recovery/saved_resident.rs"]
mod saved_resident;

fn recover_creation(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> usize {
    recover_creation_when(owner, window, cx, |_| true)
}

fn recover_creation_when(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
    mut completed: impl FnMut(&mut TestAppContext) -> bool,
) -> usize {
    let mut dispatched_frames = 0;
    let started = Instant::now();
    let mut diagnostics = 0;
    wait(
        cx,
        |cx| {
            if diagnostics < 2
                && started.elapsed() >= Duration::from_secs(if diagnostics == 0 { 3 } else { 10 })
            {
                diagnostics += 1;
                eprintln!(
                    "thread creation recovery progress: {}; candidate={}; original={:?}; release={:?}",
                    recovery_stage(owner),
                    owner.borrow().test_thread_creation_recovery_stage(),
                    window
                        .read_with(cx, |root, _| root.test_thread_confirmation_diagnostics())
                        .unwrap(),
                    window
                        .read_with(cx, |root, _| root
                            .test_thread_creation_recovery_widget_release_counts())
                        .unwrap(),
                );
            }
            cx.update(|app| {
                if RunningProcessOwner::test_dispatch_scheduled_resident_frame(owner, app) {
                    dispatched_frames += 1;
                    assert!(
                        !RunningProcessOwner::test_dispatch_scheduled_resident_frame(owner, app)
                    );
                }
                RunningProcessOwner::test_observe_running_home_failure(owner, app);
            });
            match owner.borrow().automatic_recovery_outcome().as_deref() {
                Some(InterruptedExitRecoveryOutcome::Completed) => completed(cx),
                Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => panic!(
                    "thread creation recovery unavailable: {error}; {}",
                    recovery_stage(owner)
                ),
                Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                    panic!("original thread creation recovery cancelled")
                }
                _ => false,
            }
        },
        "original thread creation did not recover in its surviving window",
    );
    dispatched_frames
}

#[gpui::test]
fn committed_creation_recovers_original_target_in_same_window_after_predecessor_disposal_failure(
    cx: &mut TestAppContext,
) {
    let (directory, original_owner, original_window, _) = selected_window(cx);
    edit_prior(original_window, "saved before failed creation", cx);
    let original_prior = selection(original_window, cx);
    let original_history = saved_resident::capture_history(&original_owner, original_prior);
    dispose_recovered(original_owner, original_window, cx);
    let (owner, window, faults) =
        saved_resident::reopen(&directory, original_prior, original_history, cx);
    owner.borrow_mut().test_enable_resident_frame_capture();
    assert!(
        !cx.update(|app| RunningProcessOwner::test_dispatch_scheduled_resident_frame(&owner, app))
    );
    let native_window = window.window_id();
    catalog::populate(&owner, 1);
    saved_resident::seed_prepublication_prior(&owner, window, faults.clone(), cx);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let prior = selection(window, cx);
    assert_eq!(prior.binding().logical_extent().logical_utf8_bytes(), 28);
    assert!(
        window
            .read_with(cx, |root, app| root
                .test_thread_creation_composer_adopted_custody_items(app))
            .unwrap()
            > 0
    );
    let (home, state) = {
        let owner = owner.borrow();
        let graph = owner.test_services().graph().unwrap();
        (graph.home().service_reference(), graph.state().clone())
    };
    let original_target = Arc::new(Mutex::new(None));
    let captured_target = original_target.clone();
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_hooks(
                None,
                None,
                Some(Box::new(move |_| {
                    let evidence = state
                        .session()
                        .capture_window_removal(&home, prior.window_id())
                        .unwrap();
                    let target = evidence.window().clone();
                    assert_ne!(target.selected_thread(), Some(prior.claim()));
                    *captured_target.lock().unwrap() = Some(target);
                    faults.fail_next(FaultPoint::BeforeReadConfirmation);
                    assert!(home.home_revision().is_err());
                    assert_eq!(
                        home.health().state(),
                        beryl_home_store::HomeHealthState::Failed
                    );
                })),
            );
        })
        .unwrap();
    confirm(&owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    recover_creation_when(&owner, window, cx, |cx| {
        let current = selection(window, cx);
        current.binding().home_generation() != prior.binding().home_generation()
            && original_target
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|target| target.selected_thread() == Some(current.claim()))
    });
    let (predecessor_release_requests, _) = window
        .read_with(cx, |root, _| {
            root.test_thread_creation_recovery_widget_release_counts()
        })
        .unwrap();
    let original_page_acknowledgements = window
        .read_with(cx, |root, _| {
            root.test_thread_creation_prepublication_page_release_acknowledgements()
        })
        .unwrap();
    assert!(
        original_page_acknowledgements > 0,
        "original Page acknowledgements={original_page_acknowledgements}, ordinary release requests={predecessor_release_requests}"
    );
    let original_target = original_target.lock().unwrap().clone().unwrap();
    let target = selection(window, cx);
    assert_eq!(window.window_id(), native_window);
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(target.window_id(), prior.window_id());
    assert_eq!(Some(target.claim()), original_target.selected_thread());
    assert_ne!(
        target.binding().home_generation(),
        prior.binding().home_generation()
    );
    assert_ne!(
        target.binding().candidate().session_id(),
        prior.binding().candidate().session_id()
    );
    assert_eq!(target.binding().logical_extent().logical_utf8_bytes(), 0);
    assert!(!owner.borrow().test_services().running_selection_pending());
    assert!(
        window
            .read_with(cx, |root, app| {
                !root.test_runtime_setup_state().0
                    && root.test_first_conversation_transcript_claim(target.claim(), app)
            })
            .unwrap()
    );
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let current = graph
            .state()
            .session()
            .capture_window_removal(graph.home(), prior.window_id())
            .unwrap();
        assert_eq!(current.window(), &original_target);
        let claim = graph
            .state()
            .session()
            .window_claim_catalog_source(graph.home(), prior.window_id())
            .unwrap()
            .claim()
            .unwrap();
        assert_eq!(claim.thread_id(), target.claim().thread_id());
        assert_eq!(claim.generation(), target.claim().generation());
        assert_eq!(claim.revision(), target.claim().revision());
        assert_eq!(claim.state(), beryl_state::ThreadClaimState::Active);
        assert!(
            graph
                .state()
                .session()
                .thread_claim_catalog_source(graph.home(), prior.claim().thread_id())
                .unwrap()
                .claim()
                .is_none()
        );
        let row = graph
            .state()
            .catalog()
            .current_row_source(
                graph.home(),
                target.claim().thread_id(),
                beryl_state::CatalogPointReadLimit::schema_maximum(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(row.row().sources().claim(), Some(target.claim().revision()));
        assert_eq!(
            row.row().facts().claim(),
            beryl_state::CatalogClaimSummary::claimed(
                prior.window_id(),
                beryl_state::CatalogClaimKind::Active,
            )
        );
        let text = graph
            .syndic()
            .current_draft_piece_text_demand(
                graph.home(),
                prior.claim().thread_id(),
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                4096,
            )
            .unwrap()
            .unwrap();
        assert_eq!(text.value().bytes(), b"saved before failed creation");
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    dispose_recovered(owner, window, cx);
    assert_reopens(&directory);
}

#[gpui::test]
fn unadmitted_creation_recovers_prior_edits_and_history_after_original_save_failure(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults) = selected_window(cx);
    let native_window = window.window_id();
    edit_prior(window, "latest unsaved prior", cx);
    let prior = selection(window, cx);
    let history = prior.binding().history();
    assert!(history.availability().undo_available());
    let original_history = saved_resident::capture_history(&owner, prior);
    let original_selection = window
        .read_with(cx, |root, app| {
            root.test_thread_creation_composer_selection(app).unwrap()
        })
        .unwrap();
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    owner.borrow_mut().test_enable_resident_frame_capture();
    assert!(
        !cx.update(|app| RunningProcessOwner::test_dispatch_scheduled_resident_frame(&owner, app))
    );
    let home = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .home()
        .service_reference();
    let interrupted = Arc::new(Mutex::new(false));
    let hook_interrupted = interrupted.clone();
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
                    *hook_interrupted.lock().unwrap() = true;
                })),
                None,
            );
        })
        .unwrap();
    confirm(&owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    let dispatched_frames = recover_creation(&owner, window, cx);
    assert!(dispatched_frames > 0);
    assert!(*interrupted.lock().unwrap());
    let restored = selection(window, cx);
    assert_eq!(window.window_id(), native_window);
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(restored.window_id(), prior.window_id());
    assert_eq!(restored.claim(), prior.claim());
    assert_ne!(
        restored.binding().home_generation(),
        prior.binding().home_generation()
    );
    assert_eq!(restored.binding().root(), prior.binding().root());
    saved_resident::assert_preserved_history(
        &original_history,
        &saved_resident::capture_history(&owner, restored),
    );
    saved_resident::assert_canonical_publication(&owner, restored, history);
    assert_eq!(
        restored.binding().history().key().session_id(),
        history.key().session_id()
    );
    assert!(
        restored
            .binding()
            .history()
            .key()
            .publication_operation_id()
            .is_some()
    );
    assert_eq!(
        restored.binding().logical_extent(),
        prior.binding().logical_extent()
    );
    assert_eq!(
        window
            .read_with(cx, |root, app| root
                .test_thread_creation_composer_selection(app)
                .unwrap())
            .unwrap(),
        original_selection
    );
    assert!(!owner.borrow().test_services().running_selection_pending());
    assert!(
        window
            .read_with(cx, |root, app| {
                root.test_first_conversation_transcript_claim(prior.claim(), app)
            })
            .unwrap()
    );
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let current = graph
            .state()
            .session()
            .capture_window_removal(graph.home(), prior.window_id())
            .unwrap();
        assert_eq!(current.window().selected_thread(), Some(prior.claim()));
        let remembered = current.window().remembered_target().unwrap();
        assert_eq!(remembered.root_id(), RootId::from_bytes([75; 16]));
        let claim = graph
            .state()
            .session()
            .window_claim_catalog_source(graph.home(), prior.window_id())
            .unwrap()
            .claim()
            .unwrap();
        assert_eq!(claim.thread_id(), prior.claim().thread_id());
        assert_eq!(claim.generation(), prior.claim().generation());
        assert_eq!(claim.revision(), prior.claim().revision());
        let text = graph
            .syndic()
            .current_draft_piece_text_demand(
                graph.home(),
                prior.claim().thread_id(),
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                4096,
            )
            .unwrap()
            .unwrap();
        assert_eq!(text.value().bytes(), b"latest unsaved prior");
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    dispose_recovered(owner, window, cx);
    assert_reopens(&directory);
}
