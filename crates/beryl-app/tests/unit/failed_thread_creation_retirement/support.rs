use super::*;

pub(super) fn service(
    fixture: &support::Fixture,
    edited: bool,
) -> Arc<MainWindowConversationComposerService> {
    let (selected, _) = fixture.claims();
    let mut host = fixture.activated_host(fixture.selected_thread, 201, 202, 1);
    if edited {
        let binding = host.binding().unwrap();
        edit_support::commit_text(
            &mut host,
            &fixture.store,
            binding,
            1,
            0,
            0,
            "saved predecessor",
            17,
            1,
        );
    }
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        slot,
    ))
}

pub(super) fn fail(fixture: &support::Fixture) {
    if fixture.store.health().state() == HomeHealthState::Healthy {
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    }
    assert!(fixture.store.home_revision().is_err());
    assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
}

pub(super) fn source(
    prior: MainWindowComposerSelectionIdentity,
) -> Box<MainWindowClaimRetirementSource> {
    Box::new(MainWindowClaimRetirementSource {
        kind: crate::main_window::MainWindowClaimRetirementKind::ThreadCreation,
        prior,
        selected: prior,
        saved: None,
        committed_target: None,
        planned_target: prior.claim(),
        receipt: None,
        completed_predecessor: None,
        completed_successor: None,
        completed_progress: None,
        mounted_successor: None,
        widget_work: None,
        release: None,
        successor_release: None,
    })
}

pub(super) fn save(
    fixture: &support::Fixture,
    service: &Arc<MainWindowConversationComposerService>,
) -> MainWindowThreadPredecessorSave {
    let mut selected = service.selected_identity().unwrap();
    let mut admission = service.begin_thread_predecessor_save(selected).unwrap();
    for _ in 0..32 {
        let (ticket, state) = match admission {
            ComposerHostFlushAdmission::Started { ticket, state }
            | ComposerHostFlushAdmission::Joined { ticket, state } => (ticket, state),
            _ => panic!("original save skipped its checkpoint"),
        };
        if state == ComposerHostFlushState::CaptureRequired {
            service
                .capture_thread_predecessor_save(
                    selected,
                    ticket,
                    fixture.assets(),
                    &fixture.marker_seals(),
                    support::operation_id(205),
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
                return service
                    .qualify_thread_predecessor_save(selected, ticket)
                    .unwrap();
            }
            ComposerHostFlushAdvance::Progress(state) => {
                admission = ComposerHostFlushAdmission::Joined { ticket, state }
            }
            ComposerHostFlushAdvance::ReconciliationPending => {}
            _ => panic!("original save failed"),
        }
    }
    panic!("save exceeded bounded fixture settlement")
}

pub(super) fn committed(
    fixture: &support::Fixture,
    selected: MainWindowComposerSelectionIdentity,
) -> SameWindowThreadCommit {
    let execution = fixture
        .storage
        .thread_execution(
            &fixture.store,
            fixture.selected_thread,
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let request = SameWindowThreadRequest::new(
        fixture.window_id,
        Some(selected.claim()),
        beryl_state::RememberedTarget::new(execution.runtime_id(), execution.root_id()),
        beryl_model::SyndicThreadId::from_bytes([250; 16]),
        beryl_model::SyndicDraftId::from_bytes([251; 16]),
        execution,
        syndic_storage::SyndicTimestamp::from_unix_millis(2000),
        syndic_storage::DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
    )
    .unwrap();
    let SameWindowThreadPreparation::Prepared(prepared) = request
        .prepare(
            &fixture.store,
            &fixture.state(),
            &fixture.storage,
            CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("edited predecessor did not prepare a new thread")
    };
    let SameWindowThreadOutcome::Settled(committed) =
        prepared.commit(&fixture.store, &fixture.state())
    else {
        panic!("original claim did not commit")
    };
    committed
}

pub(super) fn activated(
    _fixture: &support::Fixture,
    service: &Arc<MainWindowConversationComposerService>,
    committed: &SameWindowThreadCommit,
) -> crate::main_window::MainWindowComposerActivationReceipt {
    match service
        .begin_activation(
            committed.selection,
            support::activation(committed.selection.thread_id(), 210, 211, 2),
            support::operation_id(212),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        MainWindowComposerActivationAdvance::Ready(receipt) => receipt,
        _ => panic!("original successor did not activate"),
    }
}
