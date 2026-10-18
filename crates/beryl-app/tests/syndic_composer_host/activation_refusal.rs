use super::*;
use beryl_home_store::{CommandError, CommandOutcome, RevisionConflict};
use std::sync::{Arc, Mutex};
use syndic_storage::DraftEditorCandidateSessionCommandErrorV1;

#[test]
fn original_activation_cas_refusal_preserves_selector_draft_history_and_prior_host() {
    let (_home, store, storage, thread) = fixture("activation-cas-refusal", 80);
    populate(storage.clone(), &store, thread, 81);
    let mut prior = SyndicComposerHost::new(storage.clone());
    let ComposerHostActivationOutcome::Activated { binding, .. } = prior
        .test_activate(
            &store,
            activation(thread, 81, 91, Vec::new()),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("original edited candidate did not activate");
    };
    assert!(prior.is_dirty());
    let original = current(storage.clone(), &store, thread);
    let original_selector = storage
        .current_draft_piece_text_demand(&store, thread, DraftPieceTextDemandV1::Forward(0), 65_536)
        .unwrap()
        .unwrap();
    let original_session = storage
        .draft_editor_candidate_session(
            &store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap();
    assert!(matches!(
        &original_session,
        DraftEditorCandidateSessionReadOutcomeV1::Active(_)
    ));
    assert_ne!(binding.candidate().history(), original.draft().history());
    let original_text = storage
        .draft_piece_text_demand(
            &store,
            binding.root(),
            DraftPieceTextDemandV1::Forward(0),
            65_536,
        )
        .unwrap();
    assert_eq!(original_text.bytes(), "α\nβ\n".as_bytes());
    let expected_home = store.home_revision().unwrap();
    let expected_domain = storage.revision(&store).unwrap();
    let refused_session = DraftEditorCandidateSessionIdV1::from_bytes([95; 16]);
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, original.draft().id(), refused_session)
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Absent
    );

    let observed = Arc::new(Mutex::new(Vec::new()));
    let before = observed.clone();
    let mut refused = SyndicComposerHost::new(storage.clone());
    refused.test_arm_activation_before_execute_fault(move |store, storage| {
        assert_eq!(store.home_revision().unwrap(), expected_home);
        assert_eq!(storage.revision(store).unwrap(), expected_domain);
        support::bump_home_revision(storage.clone(), store, 110);
        assert_ne!(store.home_revision().unwrap(), expected_home);
        assert_ne!(storage.revision(store).unwrap(), expected_domain);
        before.lock().unwrap().push("unrelated mutation committed");
    });
    let outcome_seen = observed.clone();
    let observer_storage = storage.clone();
    let observer_original = original.clone();
    refused.test_arm_activation_after_execute_fault(move |store, outcome| {
        let CommandOutcome::NotCommitted {
            evidence:
                CommandError::Conflict {
                    conflicts_len,
                    conflicts,
                },
        } = outcome
        else {
            panic!("original activation was not refused by CAS: {outcome:?}");
        };
        assert_eq!(*conflicts_len, 2);
        assert_eq!(
            conflicts,
            &vec![
                RevisionConflict::Home {
                    expected: expected_home,
                    current: store.home_revision().unwrap(),
                },
                RevisionConflict::Domain {
                    domain: "syndic",
                    expected: expected_domain,
                    current: observer_storage.revision(store).unwrap(),
                },
            ]
        );
        assert_eq!(
            current(observer_storage.clone(), store, thread),
            observer_original
        );
        assert_eq!(
            observer_storage
                .draft_editor_candidate_session(
                    store,
                    observer_original.draft().id(),
                    refused_session
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Absent
        );
        let mut observed = outcome_seen.lock().unwrap();
        assert_eq!(&*observed, &["unrelated mutation committed"]);
        observed.push("original outcome observed before reconciliation");
    });
    let result = refused.test_activate(
        &store,
        activation(
            thread,
            95,
            96,
            vec![ComposerHostInitialDemand::Text {
                request_id: request_id(1),
                purpose: ComposerHostRequestPurpose::Viewport,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 65_536,
            }],
        ),
        &CommandCancellation::new(),
    );
    assert!(matches!(
        result,
        Err(ComposerHostError::SessionCommand(
            DraftEditorCandidateSessionCommandErrorV1::Invariant
        ))
    ));
    assert_eq!(
        &*observed.lock().unwrap(),
        &[
            "unrelated mutation committed",
            "original outcome observed before reconciliation",
        ]
    );
    assert_eq!(current(storage.clone(), &store, thread), original);
    assert_eq!(
        storage
            .current_draft_piece_text_demand(
                &store,
                thread,
                DraftPieceTextDemandV1::Forward(0),
                65_536
            )
            .unwrap()
            .unwrap(),
        original_selector
    );
    assert_eq!(
        storage
            .draft_editor_candidate_session(
                &store,
                binding.candidate().draft_id(),
                binding.candidate().session_id(),
            )
            .unwrap(),
        original_session
    );
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                binding.root(),
                DraftPieceTextDemandV1::Forward(0),
                65_536
            )
            .unwrap(),
        original_text
    );
    assert_eq!(prior.binding(), Some(binding));
    assert!(prior.is_dirty());
    assert_eq!(refused.binding(), None);
    assert!(refused.initial_responses().is_empty());
    assert_eq!(refused.pending_request_count(), 0);
    assert_eq!(refused.settlement_custody_in_use(), 0);
    assert_eq!(refused.publication_custody_count(), 0);
    assert_eq!(refused.test_publication_source_custody_count(), 0);
    assert_eq!(refused.test_last_settlement_identity_custody_count(), 0);
    assert_eq!(refused.history_status(), None);
    assert!(!refused.is_dirty());
    assert!(!refused.is_unavailable());
    assert!(matches!(
        refused.begin_request(key(binding, 1), text_request()),
        Err(ComposerHostError::OldBinding)
    ));
    assert_eq!(refused.pending_request_count(), 0);
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, original.draft().id(), refused_session)
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Absent
    );
    refused.dispose_composer_service(&store).unwrap();
    assert_eq!(refused.settlement_custody_in_use(), 0);
    assert_eq!(prior.binding(), Some(binding));
    let response = run(
        &mut prior,
        &store,
        binding,
        1,
        ComposerHostRequestKind::Text {
            target: ComposerHostReadTarget::Candidate,
            demand: DraftPieceTextDemandV1::Forward(0),
            max_bytes: 65_536,
        },
    )
    .unwrap();
    let ComposerHostResponseValue::CandidateText(text) = response else {
        panic!("original candidate read was not preserved");
    };
    assert_eq!(text.value(), &original_text);
}

#[test]
fn current_activation_admission_preserves_prepared_source_across_unrelated_write() {
    let (_home, store, storage, thread) = fixture("activation-current-unrelated", 120);
    let original = current(storage.clone(), &store, thread);
    let original_home = store.home_revision().unwrap();
    let mut host = SyndicComposerHost::new(storage.clone());
    host.test_arm_activation_before_execute_fault(|store, storage| {
        support::bump_home_revision(storage, store, 130);
    });
    let observations = Arc::new(Mutex::new(0));
    let observed = observations.clone();
    host.test_arm_activation_after_execute_fault(move |_, outcome| {
        assert!(matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        *observed.lock().unwrap() += 1;
    });
    let ComposerHostActivationOutcome::Activated {
        disposition: ComposerHostOpenDisposition::Opened,
        binding,
    } = host
        .test_activate_with_current_admission(
            &store,
            activation(thread, 140, 141, Vec::new()),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("original prepared candidate did not open");
    };
    assert_eq!(*observations.lock().unwrap(), 1);
    assert_ne!(store.home_revision().unwrap(), original_home);
    assert_eq!(current(storage.clone(), &store, thread), original);
    let DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
        .draft_editor_candidate_session(
            &store,
            original.draft().id(),
            binding.candidate().session_id(),
        )
        .unwrap()
    else {
        panic!("original prepared candidate is absent");
    };
    assert_eq!(session.thread_id(), thread);
    assert_eq!(
        session.session_id(),
        DraftEditorCandidateSessionIdV1::from_bytes([140; 16])
    );
    assert_eq!(
        session.open_operation_id(),
        DraftPieceOperationIdV1::from_bytes([141; 16])
    );
    assert_eq!(
        session.durable_base_selector_revision(),
        original.draft().revision()
    );
    assert_eq!(session.durable_base_root(), original.draft().piece_root());
    assert_eq!(session.durable_base_history(), original.draft().history());
    assert_eq!(binding.root(), original.draft().piece_root());
    assert_eq!(host.binding(), Some(binding));
    host.dispose_composer_service(&store).unwrap();
    assert_unbound_without_activation_custody(&host);
    assert_eq!(current(storage, &store, thread), original);
}

#[test]
fn current_activation_admission_refuses_changed_original_selector() {
    let (_home, store, storage, thread) = fixture("activation-current-selector", 150);
    let original = current(storage.clone(), &store, thread);
    let next_thread_revision = original.thread().revision().checked_next().unwrap();
    let advanced_thread = ThreadRecord::new(
        thread,
        SelectedPathProof::new(
            original.thread().committed_tail(),
            next_thread_revision,
            original.thread().selected_path_digest(),
        ),
        original.thread().current_draft_id(),
        original.thread().lineage(),
        original.thread().context_owner_id(),
    );
    let advanced_index = DraftByThreadRecord::new(
        thread,
        original.draft().id(),
        original.draft().revision(),
        next_thread_revision,
    );
    let expected_selector = syndic_storage::DraftEditorCurrentSelectorV1::new(
        thread,
        next_thread_revision,
        original.draft().id(),
        original.draft().revision(),
        original.draft().piece_root(),
        original.draft().history(),
    );
    let mut host = SyndicComposerHost::new(storage.clone());
    host.test_arm_activation_before_execute_fault(move |store, storage| {
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::Thread(advanced_thread)).unwrap();
        batch
            .put(FixtureRecord::DraftByThread(advanced_index))
            .unwrap();
        committed(execute(
            store,
            storage.fixture_contribution(storage.revision(store).unwrap(), batch),
        ));
    });
    let observations = Arc::new(Mutex::new(0));
    let observed = observations.clone();
    host.test_arm_activation_after_execute_fault(move |_, outcome| {
        assert!(matches!(
            outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::EmptyContribution { domain: "syndic" }
            }
        ));
        *observed.lock().unwrap() += 1;
    });
    assert!(matches!(
        host.test_activate_with_current_admission(
            &store,
            activation(thread, 160, 161, Vec::new()),
            &CommandCancellation::new(),
        ),
        Ok(ComposerHostActivationOutcome::SelectorConflict(selector)) if selector == expected_selector
    ));
    assert_eq!(*observations.lock().unwrap(), 1);
    let after = current(storage.clone(), &store, thread);
    assert_eq!(after.thread().revision(), next_thread_revision);
    assert_eq!(after.draft(), original.draft());
    assert_eq!(after.root(), original.root());
    assert_eq!(
        storage
            .draft_editor_candidate_session(
                &store,
                original.draft().id(),
                DraftEditorCandidateSessionIdV1::from_bytes([160; 16]),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Absent
    );
    assert_unbound_without_activation_custody(&host);
}

#[test]
fn current_activation_admission_preserves_cancellation_before_writer_admission() {
    let (_home, store, storage, thread) = fixture("activation-current-cancel", 170);
    let original = current(storage.clone(), &store, thread);
    let original_home = store.home_revision().unwrap();
    let original_domain = storage.revision(&store).unwrap();
    let cancellation = CommandCancellation::new();
    let cancel = cancellation.clone();
    let mut host = SyndicComposerHost::new(storage.clone());
    host.test_arm_activation_before_execute_fault(move |_, _| cancel.cancel());
    let observations = Arc::new(Mutex::new(0));
    let observed = observations.clone();
    host.test_arm_activation_after_execute_fault(move |_, outcome| {
        assert!(matches!(
            outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::CancelledBeforeAdmission
            }
        ));
        *observed.lock().unwrap() += 1;
    });
    assert!(matches!(
        host.test_activate_with_current_admission(
            &store,
            activation(thread, 180, 181, Vec::new()),
            &cancellation,
        ),
        Ok(ComposerHostActivationOutcome::Cancelled)
    ));
    assert_eq!(*observations.lock().unwrap(), 1);
    assert_eq!(store.home_revision().unwrap(), original_home);
    assert_eq!(storage.revision(&store).unwrap(), original_domain);
    assert_eq!(current(storage.clone(), &store, thread), original);
    assert_eq!(
        storage
            .draft_editor_candidate_session(
                &store,
                original.draft().id(),
                DraftEditorCandidateSessionIdV1::from_bytes([180; 16]),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Absent
    );
    assert_unbound_without_activation_custody(&host);
}

fn assert_unbound_without_activation_custody(host: &SyndicComposerHost) {
    assert_eq!(host.binding(), None);
    assert!(host.initial_responses().is_empty());
    assert_eq!(host.pending_request_count(), 0);
    assert_eq!(host.settlement_custody_in_use(), 0);
    assert_eq!(host.publication_custody_count(), 0);
    assert_eq!(host.test_publication_source_custody_count(), 0);
    assert_eq!(host.test_last_settlement_identity_custody_count(), 0);
    assert_eq!(host.history_status(), None);
    assert!(!host.is_dirty());
    assert!(!host.is_unavailable());
}
