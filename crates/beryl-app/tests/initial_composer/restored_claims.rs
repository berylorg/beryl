use super::restoration_support::*;
use super::*;
use beryl_state::ThreadClaimState;

#[test]
fn selected_editor_requires_its_exact_durably_active_claim() {
    let fixture = Fixture::new(31);
    let acquired = fixture.acquire(32);
    let window = acquired.window_id();
    let draft = acquired.draft_id();
    drop(acquired);
    begin_restore(&fixture);
    let original = paired_claim(&fixture, window);
    assert_eq!(original.state(), ThreadClaimState::Restoring);
    let (attempt, _service) = attempt(&fixture);
    let mut custody = begin(&fixture, &attempt, 33);
    open(&mut custody, &attempt);
    let failure = custody
        .prepare(&attempt, &mut config)
        .err()
        .expect("Restoring cannot bind a selected editor");
    let mut custody = failure.custody;
    assert_eq!(
        custody
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    let active = paired_claim(&fixture, window);
    assert_eq!(active.state(), ThreadClaimState::Active);
    assert_ne!(active.generation(), original.generation());
    assert_ne!(active.revision(), original.revision());
    let after = snapshot(&fixture);
    let prepared = custody
        .prepare(&attempt, &mut config)
        .unwrap_or_else(|f| panic!("{}", f.error));
    let selection = prepared.selection_identity();
    assert_eq!(selection.window_id(), window);
    assert_eq!(selection.claim(), fixture.claim(window));
    assert_eq!(selection.binding().candidate().draft_id(), draft);
    assert!(matches!(
        prepared.retire(CommandCancellation::new()),
        RestoredWindowComposerRetirement::Retired
    ));
    assert_eq!(snapshot(&fixture), after);
    assert_eq!(paired_claim(&fixture, window), active);
}

#[test]
fn sibling_activations_advance_only_their_own_records_and_keep_both_editors_valid() {
    let fixture = Fixture::new(41);
    let first = fixture.acquire(42);
    let second = fixture.acquire(43);
    let (first_window, second_window) = (first.window_id(), second.window_id());
    drop((first, second));
    begin_restore(&fixture);
    let initial = snapshot(&fixture);
    let (attempt, _service) = attempt(&fixture);
    let mut first = begin_window(&fixture, &attempt, first_window, 44);
    let mut second = begin_window(&fixture, &attempt, second_window, 47);
    open(&mut first, &attempt);
    open(&mut second, &attempt);
    let second_before = paired_claim(&fixture, second_window);
    assert_eq!(
        first
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    assert_eq!(paired_claim(&fixture, second_window), second_before);
    let first_active = paired_claim(&fixture, first_window);
    assert_eq!(
        second
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    assert_eq!(paired_claim(&fixture, first_window), first_active);
    let final_state = snapshot(&fixture);
    assert_ne!(initial.header().revision(), final_state.header().revision());
    assert_eq!(final_state.windows().len(), 2);
    for custody in [first, second] {
        let window = custody.window_id();
        let prepared = custody
            .prepare(&attempt, &mut config)
            .unwrap_or_else(|f| panic!("{}", f.error));
        assert_eq!(prepared.selection_identity().claim(), fixture.claim(window));
        assert_eq!(
            paired_claim(&fixture, window).state(),
            ThreadClaimState::Active
        );
        assert!(matches!(
            prepared.retire(CommandCancellation::new()),
            RestoredWindowComposerRetirement::Retired
        ));
    }
    assert_eq!(snapshot(&fixture), final_state);
}

#[test]
fn cancelled_claim_activation_writes_nothing_and_allows_a_fresh_explicit_retry() {
    let fixture = Fixture::new(51);
    drop(fixture.acquire(52));
    begin_restore(&fixture);
    let (attempt, _service) = attempt(&fixture);
    let mut custody = begin(&fixture, &attempt, 53);
    open(&mut custody, &attempt);
    let before = snapshot(&fixture);
    let revision = fixture.store.home_revision().unwrap();
    let cancel = CommandCancellation::new();
    cancel.cancel();
    assert!(custody.activate_claim(&attempt, &cancel).is_err());
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(snapshot(&fixture), before);
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert_eq!(
        custody
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    let active = snapshot(&fixture);
    retire(custody);
    assert_eq!(snapshot(&fixture), active);
}

#[test]
fn cancellation_at_claim_command_boundary_retains_failure_and_allows_retry() {
    let fixture = Fixture::new(81);
    drop(fixture.acquire(82));
    begin_restore(&fixture);
    let (attempt, _service) = attempt(&fixture);
    let mut custody = begin(&fixture, &attempt, 83);
    open(&mut custody, &attempt);
    let before = snapshot(&fixture);
    let revision = fixture.store.home_revision().unwrap();
    let cancel = CommandCancellation::new();
    let boundary_cancel = cancel.clone();
    custody.test_arm_before_claim_activation(move || boundary_cancel.cancel());
    assert_eq!(
        custody.activate_claim(&attempt, &cancel).unwrap(),
        RestoredClaimActivationProgress::Retry
    );
    assert!(custody.claim_activation_failure().is_some());
    assert!(custody.claim_activation_receipt().is_none());
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(snapshot(&fixture), before);
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert_eq!(
        custody
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    let active = snapshot(&fixture);
    retire(custody);
    assert_eq!(snapshot(&fixture), active);
}

#[test]
fn unsettled_claim_blocks_its_sibling_until_original_command_settles() {
    let fixture = Fixture::new(91);
    let first = fixture.acquire(92);
    let second = fixture.acquire(93);
    let (first_window, second_window) = (first.window_id(), second.window_id());
    drop((first, second));
    begin_restore(&fixture);
    let (attempt, _service) = attempt(&fixture);
    let mut first = begin_window(&fixture, &attempt, first_window, 94);
    let mut second = begin_window(&fixture, &attempt, second_window, 97);
    open(&mut first, &attempt);
    open(&mut second, &attempt);
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert_eq!(
        first
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Pending
    );
    let uncertain = snapshot(&fixture);
    assert!(
        second
            .activate_claim(&attempt, &CommandCancellation::new())
            .is_err()
    );
    assert_eq!(snapshot(&fixture), uncertain);
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    assert_eq!(
        first
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    assert_eq!(
        second
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    assert!(fixture.store.pending_reconciliations().is_empty());
    let active = snapshot(&fixture);
    retire(first);
    retire(second);
    assert_eq!(snapshot(&fixture), active);
}

#[test]
fn uncertain_activation_resumes_its_original_claim_command_before_editor_binding() {
    let fixture = Fixture::new(61);
    let acquired = fixture.acquire(62);
    let window = acquired.window_id();
    drop(acquired);
    begin_restore(&fixture);
    let (attempt, _service) = attempt(&fixture);
    let mut custody = begin(&fixture, &attempt, 63);
    open(&mut custody, &attempt);
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert_eq!(
        custody
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Pending
    );
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    let handle = fixture
        .store
        .pending_reconciliations()
        .into_iter()
        .next()
        .unwrap();
    let observed = snapshot(&fixture);
    assert_eq!(
        paired_claim(&fixture, window).state(),
        ThreadClaimState::Active
    );
    let failure = custody
        .prepare(&attempt, &mut config)
        .err()
        .expect("uncertain Active record is not settled custody");
    let mut custody = failure.custody;
    assert_eq!(
        custody
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert!(matches!(
        fixture.store.retry_reconciliation(&handle).unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(snapshot(&fixture), observed);
    let prepared = custody
        .prepare(&attempt, &mut config)
        .unwrap_or_else(|f| panic!("{}", f.error));
    assert_eq!(prepared.selection_identity().claim(), fixture.claim(window));
    assert!(matches!(
        prepared.retire(CommandCancellation::new()),
        RestoredWindowComposerRetirement::Retired
    ));
    assert_eq!(snapshot(&fixture), observed);
}

#[test]
fn retirement_settles_uncertain_claim_activation_without_deleting_the_saved_member() {
    let fixture = Fixture::new(71);
    let acquired = fixture.acquire(72);
    let draft = acquired.draft_id();
    drop(acquired);
    begin_restore(&fixture);
    let (attempt, _service) = attempt(&fixture);
    let mut custody = begin(&fixture, &attempt, 73);
    open(&mut custody, &attempt);
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert_eq!(
        custody
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Pending
    );
    let handle = fixture
        .store
        .pending_reconciliations()
        .into_iter()
        .next()
        .unwrap();
    let active = snapshot(&fixture);
    match custody.retire(CommandCancellation::new()) {
        RestoredWindowComposerRetirement::Retired => {}
        RestoredWindowComposerRetirement::Pending(failure) => retire(failure.custody),
    }
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert!(matches!(
        fixture.store.retry_reconciliation(&handle).unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(snapshot(&fixture), active);
    assert!(matches!(
        fixture.session(draft, 73),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
}
