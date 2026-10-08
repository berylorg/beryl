use super::*;
use crate::composer_host::{ComposerHostFlushAdvance, ComposerHostFlushState};
use crate::same_window_thread_acquisition::*;

#[path = "../../syndic_composer_history/support.rs"]
mod edit_support;

fn edited_predecessor(
    fixture: &support::Fixture,
    empty: bool,
) -> (
    Arc<MainWindowConversationComposerService>,
    beryl_state::WindowClaimSelection,
) {
    let (selected, target) = fixture.claims();
    let mut host = fixture.activated_host(fixture.selected_thread, 190, 191, 1);
    let binding = host.binding().unwrap();
    edit_support::commit_text(
        &mut host,
        &fixture.store,
        binding,
        1,
        0,
        0,
        "typed payload",
        13,
        1,
    );
    if empty {
        let binding = host.binding().unwrap();
        edit_support::commit_text(&mut host, &fixture.store, binding, 2, 0, 13, "", 0, 0);
    }
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    (
        Arc::new(MainWindowConversationComposerService::new(
            fixture.store.service_reference(),
            slot,
        )),
        target,
    )
}

fn save(
    fixture: &support::Fixture,
    service: &Arc<MainWindowConversationComposerService>,
) -> crate::main_window::MainWindowThreadPredecessorSave {
    let (selected, ticket) = checkpoint(fixture, service);
    service
        .qualify_thread_predecessor_save(selected, ticket)
        .unwrap()
}

fn checkpoint(
    fixture: &support::Fixture,
    service: &Arc<MainWindowConversationComposerService>,
) -> (
    MainWindowComposerSelectionIdentity,
    crate::composer_host::ComposerHostFlushTicket,
) {
    let mut selected = service.selected_identity().unwrap();
    let mut admission = service.begin_thread_predecessor_save(selected).unwrap();
    for _ in 0..32 {
        let (ticket, state) = match admission {
            ComposerHostFlushAdmission::Started { ticket, state }
            | ComposerHostFlushAdmission::Joined { ticket, state } => (ticket, state),
            _ => panic!("predecessor save skipped its exact checkpoint"),
        };
        if state == ComposerHostFlushState::CaptureRequired {
            service
                .capture_thread_predecessor_save(
                    selected,
                    ticket,
                    fixture.assets(),
                    &fixture.marker_seals(),
                    support::operation_id(195),
                    None,
                    syndic_storage::SyndicTimestamp::from_unix_millis(999),
                    &CommandCancellation::new(),
                )
                .unwrap();
        }
        let (advance, current) = service
            .advance_thread_predecessor_save(selected, ticket)
            .unwrap();
        selected = current;
        match advance {
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired) => {
                return (selected, ticket);
            }
            ComposerHostFlushAdvance::Progress(state) => {
                admission = ComposerHostFlushAdmission::Joined { ticket, state }
            }
            ComposerHostFlushAdvance::ReconciliationPending => {}
            _ => panic!("predecessor save failed"),
        }
    }
    panic!("predecessor save exceeded bounded settlement")
}

fn request(
    fixture: &support::Fixture,
    selected: beryl_state::WindowClaimSelection,
) -> SameWindowThreadRequest {
    let source = fixture
        .storage
        .thread_execution(
            &fixture.store,
            fixture.selected_thread,
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let execution = source.execution().clone();
    SameWindowThreadRequest::new(
        fixture.window_id,
        Some(selected),
        beryl_state::RememberedTarget::new(execution.runtime_id(), execution.root_id()),
        beryl_model::SyndicThreadId::from_bytes([250; 16]),
        beryl_model::SyndicDraftId::from_bytes([251; 16]),
        execution,
        syndic_storage::SyndicTimestamp::from_unix_millis(2000),
        syndic_storage::DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
    )
    .unwrap()
}

#[test]
fn current_typed_then_removed_editor_releases_original_saved_fence_without_disposal() {
    let fixture = support::Fixture::new("current-empty-save", 101);
    let (service, _) = edited_predecessor(&fixture, true);
    let proof = save(&fixture, &service);
    let selected = proof.selected();
    let before = fixture.store.home_revision().unwrap();
    assert!(matches!(
        request(&fixture, selected.claim())
            .prepare(
                &fixture.store,
                &fixture.state(),
                &fixture.storage,
                CommandCancellation::new()
            )
            .unwrap(),
        SameWindowThreadPreparation::Current { .. }
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    let head = fixture
        .storage
        .draft_editor_candidate_session(
            &fixture.store,
            selected.binding().candidate().draft_id(),
            selected.binding().candidate().session_id(),
        )
        .unwrap();
    assert!(proof.release(&fixture.state()).is_ok());
    assert_eq!(service.selected_identity(), Some(selected));
    assert_eq!(
        fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                selected.binding().candidate().draft_id(),
                selected.binding().candidate().session_id()
            )
            .unwrap(),
        head
    );
    assert!(service.begin_thread_predecessor_save(selected).is_ok());
}

#[test]
fn fresh_creation_noncommit_preserves_dirty_editor_and_releases_saved_fence() {
    let fixture = support::Fixture::new("new-thread-noncommit-save", 111);
    let (service, _) = edited_predecessor(&fixture, false);
    let proof = save(&fixture, &service);
    let selected = proof.selected();
    let cancellation = CommandCancellation::new();
    let SameWindowThreadPreparation::Prepared(prepared) = request(&fixture, selected.claim())
        .prepare(
            &fixture.store,
            &fixture.state(),
            &fixture.storage,
            cancellation.clone(),
        )
        .unwrap()
    else {
        panic!();
    };
    cancellation.cancel();
    assert!(matches!(
        prepared.commit(&fixture.store, &fixture.state()),
        SameWindowThreadOutcome::NotCommitted(_)
    ));
    assert!(proof.release(&fixture.state()).is_ok());
    assert_eq!(service.selected_identity(), Some(selected));
    assert_eq!(
        fixture
            .current_draft(fixture.selected_thread)
            .root()
            .reference()
            .summary()
            .logical_utf8_bytes(),
        13
    );
    assert!(service.begin_thread_predecessor_save(selected).is_ok());
}

#[test]
fn committed_fresh_editor_adopts_original_save_without_second_publication() {
    let fixture = support::Fixture::new("new-thread-save-adoption", 121);
    let (service, _) = edited_predecessor(&fixture, false);
    let proof = save(&fixture, &service);
    let selected = proof.selected();
    let SameWindowThreadPreparation::Prepared(prepared) = request(&fixture, selected.claim())
        .prepare(
            &fixture.store,
            &fixture.state(),
            &fixture.storage,
            CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!();
    };
    let SameWindowThreadOutcome::Settled(commit) =
        prepared.commit(&fixture.store, &fixture.state())
    else {
        panic!();
    };
    let MainWindowComposerActivationAdvance::Ready(receipt) = service
        .begin_activation(
            commit.selection,
            support::activation(commit.selection.thread_id(), 192, 193, 2),
            support::operation_id(194),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!();
    };
    let before = fixture.store.home_revision().unwrap();
    assert!(service.begin_claim_publication_save(receipt).is_err());
    assert!(proof.validate().is_ok());
    assert!(proof.adopt(receipt, commit.selection).is_ok());
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert_eq!(service.selected_identity(), Some(selected));
    assert!(service.begin_claim_publication_save(receipt).is_err());
    let advance = service.advance_claim_publication_source(receipt).unwrap();
    assert!(
        matches!(advance.advance, MainWindowComposerPublishAdvance::WidgetReleaseRequired(current) if current == selected)
    );
    assert_eq!(fixture.store.home_revision().unwrap(), before);
}

struct ProcessSource {
    reader: crate::app_services::PublishedRunningThreadsReader,
    service: crate::cas_projection::ProjectionConnectionService,
    _sessions: crate::cas_projection::ScheduledExecutionSessions,
    _attention: Arc<crate::lifecycle_attention::ProcessLifecycleAttentionPool>,
    lifetime: Arc<()>,
    windows: crate::window_acquisition::RuntimeBackedWindowProcessRegistry,
    _window: crate::window_acquisition::RuntimeBackedWindowMainWindowReservation,
}
impl ProcessSource {
    fn new(fixture: &support::Fixture) -> Self {
        use crate::cas_projection::*;
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let service = ProjectionConnectionService::new_borrowed_for_test(
            Default::default(),
            &fixture.store,
            fixture.storage.clone(),
            ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
            Box::new(provider),
        )
        .unwrap();
        let attention = Arc::new(crate::lifecycle_attention::ProcessLifecycleAttentionPool::new());
        let lifetime = Arc::new(());
        let reader = crate::app_services::PublishedRunningThreadsReader::for_test(
            &service,
            &sessions,
            fixture.state(),
            Arc::new(fixture.store.service_reference()),
            fixture.storage.clone(),
            &attention,
            &lifetime,
        );
        let windows = crate::window_acquisition::RuntimeBackedWindowProcessRegistry::new(
            crate::process_admission::ProcessAdmissionGate::new(),
        );
        let window = windows.reserve_main_window(fixture.window_id).unwrap();
        Self {
            reader,
            service,
            _sessions: sessions,
            _attention: attention,
            lifetime,
            windows,
            _window: window,
        }
    }
    fn lease(
        &self,
        fixture: &support::Fixture,
    ) -> Arc<crate::window_acquisition::WindowSelectionLease> {
        Arc::new(
            self.windows
                .admit_selection(&[fixture.window_id], fixture.window_id)
                .unwrap(),
        )
    }
}

#[test]
fn cancelled_preparation_retains_original_saved_editor_and_selection_lease_until_exact_release() {
    let fixture = support::Fixture::new("cancelled-thread-preparation", 131);
    let (composer, _) = edited_predecessor(&fixture, false);
    let saved = save(&fixture, &composer);
    let selected = saved.selected();
    let source = ProcessSource::new(&fixture);
    let lease = source.lease(&fixture);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let failure = match source.reader.prepare_thread_creation(
        request(&fixture, selected.claim()),
        lease,
        saved,
        cancellation,
    ) {
        Err(failure) => failure,
        Ok(_) => panic!("cancelled preparation succeeded"),
    };
    assert!(failure.error().contains("cancelled"));
    assert!(source.windows.test_close_is_blocked(&[fixture.window_id]));
    assert!(composer.begin_thread_predecessor_save(selected).is_ok());
    assert!(failure.release().is_ok());
    assert!(!source.windows.test_close_is_blocked(&[fixture.window_id]));
    assert_eq!(composer.selected_identity(), Some(selected));
    assert!(composer.begin_thread_predecessor_save(selected).is_ok());
    let _ = source.service.close().unwrap();
}

#[test]
fn current_preparation_releases_saved_editor_and_process_lease_together() {
    let fixture = support::Fixture::new("current-thread-preparation", 141);
    let (composer, _) = edited_predecessor(&fixture, true);
    let saved = save(&fixture, &composer);
    let selected = saved.selected();
    let source = ProcessSource::new(&fixture);
    let current = match source.reader.prepare_thread_creation(
        request(&fixture, selected.claim()),
        source.lease(&fixture),
        saved,
        CommandCancellation::new(),
    ) {
        Ok(crate::app_services::PublishedSameWindowThreadPreparation::Current(current)) => current,
        _ => panic!("current preparation did not preserve its owner"),
    };
    assert_eq!(current.window.selected_thread(), Some(selected.claim()));
    assert!(source.windows.test_close_is_blocked(&[fixture.window_id]));
    assert!(current.release().is_ok());
    assert!(!source.windows.test_close_is_blocked(&[fixture.window_id]));
    assert_eq!(composer.selected_identity(), Some(selected));
    let _ = source.service.close().unwrap();
}

#[test]
fn published_operation_suppresses_repeat_commit_and_retains_original_result_after_source_retirement()
 {
    let fixture = support::Fixture::new("retired-thread-operation", 151);
    let (composer, _) = edited_predecessor(&fixture, false);
    let saved = save(&fixture, &composer);
    let selected = saved.selected();
    let source = ProcessSource::new(&fixture);
    let mut operation = match source.reader.prepare_thread_creation(
        request(&fixture, selected.claim()),
        source.lease(&fixture),
        saved,
        CommandCancellation::new(),
    ) {
        Ok(crate::app_services::PublishedSameWindowThreadPreparation::Prepared(operation)) => {
            operation
        }
        _ => panic!("fresh operation not prepared"),
    };
    operation.commit().unwrap();
    assert!(matches!(
        operation.outcome(),
        Some(SameWindowThreadOutcome::Settled(_))
    ));
    let before = fixture.store.home_revision().unwrap();
    assert!(operation.commit().is_err());
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert!(source.windows.test_close_is_blocked(&[fixture.window_id]));
    drop(source.lifetime);
    assert!(operation.validate_publication().is_err());
    assert!(operation.reconcile().is_err());
    let operation = match operation.release_noncommit() {
        Err((operation, _)) => operation,
        Ok(()) => panic!("committed predecessor was released"),
    };
    assert!(matches!(
        operation.outcome(),
        Some(SameWindowThreadOutcome::Settled(_))
    ));
    assert_eq!(composer.selected_identity(), Some(selected));
    drop(operation);
    let _ = source.service.close().unwrap();
}

#[test]
fn saved_barrier_issues_one_proof_and_released_host_rejects_the_original_ticket() {
    let fixture = support::Fixture::new("sole-saved-proof", 161);
    let (composer, _) = edited_predecessor(&fixture, false);
    let (selected, ticket) = checkpoint(&fixture, &composer);
    let mut slot = composer.slot.lock().unwrap();
    let saved = slot
        .qualify_thread_predecessor_save(&fixture.store, selected, ticket)
        .unwrap();
    assert!(
        slot.qualify_thread_predecessor_save(&fixture.store, selected, ticket)
            .is_err()
    );
    assert!(
        slot.validate_thread_predecessor_save(&fixture.store, selected, saved)
            .is_ok()
    );
    slot.release_thread_predecessor_save(&fixture.store, selected, saved)
        .unwrap();
    assert!(
        slot.validate_thread_predecessor_save(&fixture.store, selected, saved)
            .is_err()
    );
    assert!(
        slot.selected_host()
            .unwrap()
            .validate_selection_save(&fixture.store, saved)
            .is_err()
    );
    assert!(
        slot.qualify_thread_predecessor_save(&fixture.store, selected, ticket)
            .is_err()
    );
}

#[test]
fn source_drift_preparation_failure_retains_saved_proof_and_lease_when_release_is_unauthorized() {
    use beryl_state::ReplaceWindowClaim;
    let fixture = support::Fixture::new("drifted-thread-preparation", 171);
    let (composer, _) = edited_predecessor(&fixture, false);
    let saved = save(&fixture, &composer);
    let selected = saved.selected();
    let source = ProcessSource::new(&fixture);
    let state = fixture.state();
    let session = state.session();
    let removal = session
        .capture_window_removal(&fixture.store, fixture.window_id)
        .unwrap();
    let mut command = beryl_home_store::HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(session.replace_claim(
            session.revision(&fixture.store).unwrap(),
            ReplaceWindowClaim::new(
                removal.header().revision(),
                fixture.window_id,
                removal.window().revision(),
                Some(selected.claim()),
                removal.window().remembered_target().unwrap(),
                fixture.target_thread,
            ),
        ))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    let failure = match source.reader.prepare_thread_creation(
        request(&fixture, selected.claim()),
        source.lease(&fixture),
        saved,
        CommandCancellation::new(),
    ) {
        Err(failure) => failure,
        Ok(_) => panic!("drifted preparation succeeded"),
    };
    let failure = match failure.release() {
        Err(failure) => failure,
        Ok(()) => panic!("drifted predecessor was released"),
    };
    assert!(failure.error().contains("durable claim"));
    assert!(source.windows.test_close_is_blocked(&[fixture.window_id]));
    assert_eq!(composer.selected_identity(), Some(selected));
    drop(failure);
    let _ = source.service.close().unwrap();
}

#[test]
fn uncertain_operation_retains_saved_editor_and_lease_through_terminal_collision() {
    let fixture = support::Fixture::new("uncertain-thread-collision", 181);
    let (composer, _) = edited_predecessor(&fixture, false);
    let saved = save(&fixture, &composer);
    let selected = saved.selected();
    let source = ProcessSource::new(&fixture);
    let mut operation = match source.reader.prepare_thread_creation(
        request(&fixture, selected.claim()),
        source.lease(&fixture),
        saved,
        CommandCancellation::new(),
    ) {
        Ok(crate::app_services::PublishedSameWindowThreadPreparation::Prepared(operation)) => {
            operation
        }
        _ => panic!("fresh operation not prepared"),
    };
    fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
    operation.commit().unwrap();
    assert!(matches!(
        operation.outcome(),
        Some(SameWindowThreadOutcome::Pending(_))
    ));
    let mut operation = match operation.release_noncommit() {
        Err((operation, _)) => operation,
        Ok(()) => panic!("uncertain predecessor was released"),
    };
    assert!(source.windows.test_close_is_blocked(&[fixture.window_id]));
    let state = fixture.state();
    let session = state.session();
    let removal = session
        .capture_window_removal(&fixture.store, fixture.window_id)
        .unwrap();
    let mut command = beryl_home_store::HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(session.replace_claim(
            session.revision(&fixture.store).unwrap(),
            beryl_state::ReplaceWindowClaim::new(
                removal.header().revision(),
                fixture.window_id,
                removal.window().revision(),
                removal.window().selected_thread(),
                removal.window().remembered_target().unwrap(),
                fixture.selected_thread,
            ),
        ))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    operation.reconcile().unwrap();
    assert!(
        matches!(operation.outcome(), Some(SameWindowThreadOutcome::Unavailable(pending)) if matches!(pending.problem(), SameWindowThreadError::Collision))
    );
    let operation = match operation.release_noncommit() {
        Err((operation, _)) => operation,
        Ok(()) => panic!("unavailable predecessor was released"),
    };
    assert!(source.windows.test_close_is_blocked(&[fixture.window_id]));
    assert_eq!(composer.selected_identity(), Some(selected));
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    drop(operation);
    let _ = source.service.close().unwrap();
}
