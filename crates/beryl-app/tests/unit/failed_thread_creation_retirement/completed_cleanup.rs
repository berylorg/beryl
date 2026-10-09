use super::*;

#[test]
fn healthy_completed_successor_settles_exact_receipt_before_one_new_editor_attempt() {
    let fixture = support::Fixture::new("healthy-completed-successor", 121);
    let service = service(&fixture, true);
    let saved = save(&fixture, &service);
    let prior = saved.selected();
    let committed = committed(&fixture, prior);
    let cancellation = CommandCancellation::new();
    let cut = cancellation.clone();
    service
        .slot
        .lock()
        .unwrap()
        .test_arm_activation_after_open_fault(move |_, _| cut.cancel());
    assert!(matches!(
        service
            .begin_thread_creation_activation(
                committed.selection,
                support::activation(committed.selection.thread_id(), 210, 211, 2),
                support::operation_id(212),
                &cancellation
            )
            .unwrap(),
        MainWindowComposerActivationAdvance::Cancelled
    ));
    assert_eq!(fixture.store.health().state(), HomeHealthState::Healthy);
    assert!(service.slot.lock().unwrap().pending_receipt().is_none());
    let completed = service
        .take_completed_thread_successor_cleanup(committed.selection)
        .unwrap()
        .unwrap();
    let original = completed.selection();
    let foreign = support::Fixture::new("foreign-completed-successor", 131);
    let (completed, _) = completed
        .settle_current(&foreign.store, &foreign.storage)
        .err()
        .unwrap();
    assert_eq!(completed.selection(), original);
    assert_eq!(
        completed.receipt().target_thread(),
        committed.selection.thread_id()
    );
    let revision = fixture.store.home_revision().unwrap();
    let progress = service
        .settle_completed_thread_successor_cleanup(completed)
        .ok()
        .unwrap();
    assert_eq!(progress.selection(), original);
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    let MainWindowComposerActivationAdvance::Ready(receipt) = service
        .begin_thread_creation_activation(
            committed.selection,
            support::activation(committed.selection.thread_id(), 220, 221, 3),
            support::operation_id(222),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("one fixed target editor attempt did not activate")
    };
    let saved = saved.adopt(receipt, committed.selection).ok().unwrap();
    let seals = fixture.marker_seals();
    fail(&fixture);
    let markers = seals.capture_failed_home(&fixture.store).unwrap();
    let mut source = source(prior);
    source.saved = Some(saved);
    source.receipt = Some(receipt);
    source.committed_target = Some(committed.selection);
    source.planned_target = committed.selection;
    source.completed_progress = Some(progress);
    let mut retirement = service
        .retire_failed_claim_cleanup(source, &markers)
        .unwrap_or_else(|(_, _, error)| panic!("original completed cleanup retirement: {error}"));
    drop(markers);
    drop(seals);
    let (_directory, store, _) = fixture.into_store();
    let mut candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    assert!(
        retirement
            .settle_predecessor_publication(&mut candidate, &storage)
            .unwrap()
    );
    retirement
        .accept_committed_claim(&committed, &mut candidate, &state)
        .unwrap();
    assert!(retirement.validate_complete_retirement().is_err());
    retirement
        .accept_predecessor_widget_release(prior, &[])
        .unwrap();
    retirement
        .settle_remaining_cleanup(&mut candidate, &storage)
        .unwrap();
    retirement.validate_complete_retirement().unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    retirement
        .settle_remaining_cleanup(&mut candidate, &storage)
        .unwrap();
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
}

#[test]
fn failed_generation_preserves_unsettled_completed_successor_cap_for_candidate_validation() {
    let fixture = support::Fixture::new("failed-completed-successor", 141);
    let service = service(&fixture, true);
    let saved = save(&fixture, &service);
    let prior = saved.selected();
    let committed = committed(&fixture, prior);
    let cancellation = CommandCancellation::new();
    let cut = cancellation.clone();
    service
        .slot
        .lock()
        .unwrap()
        .test_arm_activation_after_open_fault(move |_, _| cut.cancel());
    assert!(matches!(
        service
            .begin_thread_creation_activation(
                committed.selection,
                support::activation(committed.selection.thread_id(), 230, 231, 2),
                support::operation_id(232),
                &cancellation
            )
            .unwrap(),
        MainWindowComposerActivationAdvance::Cancelled
    ));
    let completed = service
        .take_completed_thread_successor_cleanup(committed.selection)
        .unwrap()
        .unwrap();
    let seals = fixture.marker_seals();
    fail(&fixture);
    let (completed, _) = service
        .settle_completed_thread_successor_cleanup(completed)
        .err()
        .unwrap();
    assert!(
        service
            .begin_thread_creation_activation(
                committed.selection,
                support::activation(committed.selection.thread_id(), 240, 241, 3),
                support::operation_id(242),
                &CommandCancellation::new()
            )
            .is_err()
    );
    let saved = saved.retire_failed_home().ok().unwrap();
    let markers = seals.capture_failed_home(&fixture.store).unwrap();
    let mut source = source(prior);
    source.saved = Some(saved);
    source.committed_target = Some(committed.selection);
    source.planned_target = committed.selection;
    source.completed_successor = Some(completed);
    let mut retirement = service
        .retire_failed_claim_cleanup(source, &markers)
        .unwrap_or_else(|(_, _, error)| panic!("original completed cleanup retirement: {error}"));
    drop(markers);
    drop(seals);
    let (_directory, store, _) = fixture.into_store();
    let mut candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    assert!(
        retirement
            .settle_predecessor_publication(&mut candidate, &storage)
            .unwrap()
    );
    retirement
        .accept_committed_claim(&committed, &mut candidate, &state)
        .unwrap();
    retirement
        .accept_predecessor_widget_release(prior, &[])
        .unwrap();
    retirement
        .settle_remaining_cleanup(&mut candidate, &storage)
        .unwrap();
    retirement.validate_complete_retirement().unwrap();
}
