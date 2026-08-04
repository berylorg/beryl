fn phase83_establish_pending_ordinary(
    home: &HomeStore,
    storage: SyndicStorage,
    state: BerylState,
    thread_id: SyndicThreadId,
    seed: u8,
    execution: ExecutionBinding,
    cas_thread_id: CasThreadId,
) -> (BindingRevision, CasLineageProof) {
    phase83_execute(
        home,
        storage.create_thread(
            storage.revision(home).unwrap(),
            CreateThread::ordinary(
                thread_id,
                SyndicDraftId::from_bytes([seed.wrapping_add(3); 16]),
                execution.clone(),
                SyndicTimestamp::from_unix_millis(1),
            ),
        ),
    );
    let prepared = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text("Phase 83 durable input").unwrap()]).unwrap(),
    )
    .unwrap();
    phase83_execute(
        home,
        storage.begin_content(
            storage.revision(home).unwrap(),
            ContentBuild::from_prepared(&prepared),
        ),
    );
    let mut manifest = prepared.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, &prepared).unwrap() {
        manifest = append.next_manifest().clone();
        phase83_execute(
            home,
            storage.append_content(storage.revision(home).unwrap(), append),
        );
    }
    let current = storage
        .current_draft(home, thread_id, phase83_point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, SyndicTimestamp::from_unix_millis(2))
            .unwrap()
    else {
        panic!("Phase 83 fixture content must update the initial draft")
    };
    phase83_execute(
        home,
        storage.update_draft_payload(storage.revision(home).unwrap(), update),
    );
    let current = storage
        .current_draft(home, thread_id, phase83_point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(home, thread_id, phase83_point_limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        thread_id,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([seed.wrapping_add(4); 16]),
        SyndicItemId::from_bytes([seed.wrapping_add(5); 16]),
        None,
        SyndicTimestamp::from_unix_millis(3),
    );
    home.execute(idle_submission_command(home, storage, state.assets(), submission).unwrap())
        .unwrap();

    let thread = storage
        .thread(home, thread_id, phase83_point_limit())
        .unwrap()
        .unwrap();
    let selected = SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    );
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented.clone()).unwrap();
    let binding = storage
        .current_binding(home, thread_id, phase83_point_limit())
        .unwrap()
        .unwrap();
    phase83_execute(
        home,
        storage.publish_valid_binding(
            storage.revision(home).unwrap(),
            PublishValidBinding::new(
                thread_id,
                binding.binding().revision(),
                selected,
                execution,
                cas_thread_id,
                represented,
                CasNativeTurnCount::ZERO,
                ConversationToolRegistry::canonical().profile(),
                lineage,
            ),
        ),
    );
    let binding = storage
        .current_binding(home, thread_id, phase83_point_limit())
        .unwrap()
        .unwrap();
    (binding.binding().revision(), lineage)
}

fn phase83_execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    home.execute(command).unwrap();
}

fn phase83_point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn phase83_assert_counts(
    ledger: &AdoptedProjectionCandidateReauthenticationLedger,
    unprocessed: usize,
    rejected: usize,
    accepted: usize,
    disposed: usize,
) {
    let metadata = ledger.metadata();
    assert_eq!(metadata.unprocessed_count(), unprocessed);
    assert_eq!(metadata.rejected_count(), rejected);
    assert_eq!(metadata.accepted_count(), accepted);
    assert_eq!(metadata.disposed_count(), disposed);
}

fn phase83_assert_retryable_fact_rejection(
    fixture: Phase83Fixture,
    expected: ProjectionCandidateReauthenticationReason,
) {
    let expected_registry = fixture.registry_before.clone();
    phase83_assert_retryable_fact_rejection_with_registry(fixture, expected, expected_registry);
}

fn phase83_assert_retryable_fact_rejection_with_registry(
    mut fixture: Phase83Fixture,
    expected: ProjectionCandidateReauthenticationReason,
    expected_registry: Vec<
        crate::cas_projection::connection::registry::LoadedRegistryRecoveryObservation,
    >,
) {
    let candidates = fixture.candidate_ids();
    assert_eq!(candidates.len(), 1);
    let candidate = candidates[0];
    let stable_identity = fixture.stable_identity;
    let home_revision = fixture.home.home_revision().unwrap();
    let replacement_workers = fixture.replacement_workers;
    let metadata = fixture.ledger().metadata();
    assert_eq!(metadata.adoption().candidate_count(), 1);
    assert_eq!(metadata.connection_owner_count(), 1);

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();

    assert_eq!(
        outcome.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(outcome.rejection_reason(), Some(expected));
    assert_eq!(fixture.ledger().metadata().terminal_reason(), None);
    assert_eq!(fixture.ledger().metadata().connection_owner_count(), 1);
    phase83_assert_counts(fixture.ledger(), 0, 1, 0, 0);
    assert_eq!(fixture.registry_now(), expected_registry);
    assert_eq!(fixture.connection.identity_observation(), stable_identity);
    assert_eq!(fixture.home.home_revision().unwrap(), home_revision);
    assert_eq!(
        fixture
            .ledger()
            .replacement_worker_diagnostics_for_test()
            .active(),
        replacement_workers
    );

    let disposition = fixture.ledger_mut().dispose_candidate(candidate).unwrap();
    assert_eq!(disposition.candidate_id(), candidate);
    phase83_assert_counts(fixture.ledger(), 0, 0, 0, 1);
    assert_eq!(fixture.ledger().metadata().connection_owner_count(), 1);
    assert!(fixture.registry_now().is_empty());
    assert_eq!(fixture.connection.identity_observation(), stable_identity);
    assert_eq!(fixture.home.home_revision().unwrap(), home_revision);
    assert_eq!(
        fixture
            .ledger()
            .replacement_worker_diagnostics_for_test()
            .active(),
        replacement_workers - 1
    );

    let converged = fixture.take_ledger().seal().unwrap();
    assert_eq!(converged.accepted_candidate_count(), 0);
    assert_eq!(converged.metadata().connection_count(), 1);
    drop(converged);
    fixture.close();
}

fn phase83_assert_terminal_ledger(
    fixture: &mut Phase83Fixture,
    reason: TerminalAdoptedProjectionConnectionServiceReason,
) {
    let candidate_ids = fixture.candidate_ids();
    let metadata = fixture.ledger().metadata();
    assert_eq!(metadata.terminal_reason(), Some(reason));
    assert_eq!(metadata.unprocessed_count(), 0);
    assert_eq!(metadata.rejected_count(), candidate_ids.len());
    assert_eq!(metadata.accepted_count(), 0);
    assert_eq!(metadata.disposed_count(), 0);
    assert!(!metadata.is_ready_to_seal());
    for candidate_id in &candidate_ids {
        let candidate = fixture.ledger().candidate(*candidate_id).unwrap();
        assert_eq!(
            candidate.status(),
            ProjectionCandidateReauthenticationStatus::Rejected
        );
        assert_eq!(
            candidate.rejection_reason(),
            Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
                reason
            ))
        );
    }
    if let Some(candidate_id) = candidate_ids.first().copied() {
        assert_eq!(
            fixture
                .ledger_mut()
                .reauthenticate_candidate(candidate_id),
            Err(ProjectionCandidateLedgerAccessError::LedgerTerminal)
        );
        assert_eq!(
            fixture.ledger_mut().dispose_candidate(candidate_id),
            Err(ProjectionCandidateLedgerAccessError::LedgerTerminal)
        );
    }
}

fn phase83_take_terminal_service(
    fixture: &mut Phase83Fixture,
    reason: TerminalAdoptedProjectionConnectionServiceReason,
) -> TerminalAdoptedProjectionConnectionService {
    let terminal = fixture
        .take_ledger()
        .seal()
        .unwrap_err()
        .into_terminal()
        .unwrap();
    assert_eq!(terminal.reason(), reason);
    assert_eq!(terminal.metadata().terminal_reason(), Some(reason));
    terminal
}
