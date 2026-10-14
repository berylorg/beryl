use super::*;
use beryl_home_store::HomeHealthState;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn execution_counter(run: &mut SelectedRun) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    run.host
        .test_arm_mutation_before_execute_fault(move |_, _| {
            observed.fetch_add(1, Ordering::SeqCst);
        });
    count
}

fn assert_retained(
    run: &SelectedRun,
    request: &MutationPageRequest,
    custody: (u64, bool),
    handle: bool,
) {
    assert_eq!(run.host.test_mutation_in_flight_custody(), Some(custody));
    assert_eq!(
        run.host.test_mutation_in_flight_reconciliation_handle(),
        Some(handle)
    );
    assert_eq!(run.host.binding(), Some(run.binding));
    assert_eq!(
        run.host.mutation_status(),
        Some(ComposerHostMutationStatus::Admitted)
    );
    assert_eq!(request.page().payload_owner_count(), 2);
}

fn expect_reconciliation_failure(result: Result<MutationPageAcceptance, ComposerHostError>) {
    assert!(
        matches!(
            &result,
            Err(ComposerHostError::MutationStaging(
                syndic_storage::DraftMutationStagingErrorV1::Reconciliation(_)
            ))
        ),
        "expected original page reconciliation failure, got {result:?}"
    );
}

fn expect_read_failure(result: Result<MutationPageAcceptance, ComposerHostError>) {
    assert!(
        matches!(
            &result,
            Err(ComposerHostError::MutationStaging(
                syndic_storage::DraftMutationStagingErrorV1::Read(_)
            ))
        ),
        "expected exact page read failure, got {result:?}"
    );
}

#[test]
fn healthy_page_reconciliation_and_exact_read_retry_acknowledge_target_without_reexecution() {
    let mut run = selected_run("mapped-page-flight-target-retry", 121);
    let (pass, page, finish) = admitted(&mut run, "é");
    stage_sources(&mut run, pass, false);
    let request = MutationPageRequest::new(page).with_pass(pass);
    run.fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    run.fixture
        .faults
        .fail_times(FaultPoint::BeforeReconciliationSnapshot, 2);
    expect_reconciliation_failure(run.host.stage_mutation_page(
        &run.fixture.store,
        request.clone(),
        Box::new([]),
    ));
    let custody = run.host.test_mutation_in_flight_custody().unwrap();
    assert_retained(&run, &request, custody, true);
    assert!(matches!(
        super::super::predecessor_reads::read_markers(
            &mut run.host,
            &run.fixture.store,
            run.binding,
            70_000,
            2,
            beryl_app::composer_host::ComposerHostRequestPurpose::MutationPredecessor,
        ),
        Err(ComposerHostError::MutationPending)
    ));
    assert_retained(&run, &request, custody, true);
    assert_eq!(run.fixture.store.health().state(), HomeHealthState::Healthy);
    let executions = execution_counter(&mut run);
    let different = MutationPage::new(
        request.page().key(),
        request.page().next_cursor(),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "different".into(),
        }],
    )
    .unwrap();
    assert!(matches!(
        run.host.stage_mutation_page(
            &run.fixture.store,
            MutationPageRequest::new(different).with_pass(pass),
            Box::new([])
        ),
        Err(ComposerHostError::MutationPending)
    ));
    assert_retained(&run, &request, custody, true);
    expect_reconciliation_failure(run.host.stage_mutation_page(
        &run.fixture.store,
        request.clone(),
        Box::new([]),
    ));
    assert_retained(&run, &request, custody, true);
    assert!(run.host.test_fail_mutation_staging_page_exact_read());
    expect_read_failure(run.host.stage_mutation_page(
        &run.fixture.store,
        request.clone(),
        Box::new([]),
    ));
    assert_retained(&run, &request, custody, true);
    assert_eq!(run.fixture.store.health().state(), HomeHealthState::Healthy);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert!(matches!(
        run.host
            .stage_mutation_page(&run.fixture.store, request.clone(), Box::new([]))
            .unwrap(),
        MutationPageAcceptance::Accepted { .. }
    ));
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert_eq!(request.page().payload_owner_count(), 1);
    assert_eq!(run.host.test_mutation_in_flight_custody(), None);
    super::super::predecessor_reads::assert_predecessor_markers(
        &mut run.host,
        &run.fixture.storage,
        &run.fixture.store,
        run.binding,
        70_001,
        2,
        run.removals
            .map(|target| InlineObjectNeighbor::new(target.id(), target.order())),
    );
    assert_eq!(
        run.host
            .stage_mutation_page(&run.fixture.store, request, Box::new([]))
            .unwrap(),
        MutationPageAcceptance::Replay
    );
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    run.host
        .finish_mutation_input(&run.fixture.store, finish)
        .unwrap();
    let (committed, _) = commit_mapped(&mut run.host, &run.fixture.store, finish.key());
    assert_eq!(
        candidate_text(run.fixture.storage.clone(), &run.fixture.store, committed),
        "AéE".as_bytes()
    );
    assert_eq!(committed.root().summary().marker_count(), 2);
}

#[test]
fn cancellation_resolves_original_target_flight_before_electing_noncommit() {
    let mut run = selected_run("mapped-page-flight-target-cancel", 131);
    let (pass, page, finish) = admitted(&mut run, "Q");
    stage_sources(&mut run, pass, false);
    let request = MutationPageRequest::new(page).with_pass(pass);
    run.fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    run.fixture
        .faults
        .fail_times(FaultPoint::BeforeReconciliationSnapshot, 2);
    expect_reconciliation_failure(run.host.stage_mutation_page(
        &run.fixture.store,
        request.clone(),
        Box::new([]),
    ));
    let custody = run.host.test_mutation_in_flight_custody().unwrap();
    assert_retained(&run, &request, custody, true);
    let executions = execution_counter(&mut run);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        run.host.execute_mutation(
            &run.fixture.store,
            MutationCommitRequest::new(finish.key(), MutationIdentity::ROOT),
            &cancellation
        ),
        Err(ComposerHostError::MutationStaging(
            syndic_storage::DraftMutationStagingErrorV1::Reconciliation(_)
        ))
    ));
    assert_retained(&run, &request, custody, true);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert_eq!(run.fixture.store.health().state(), HomeHealthState::Healthy);
    cancel(&mut run, finish.key());
    assert_eq!(request.page().payload_owner_count(), 1);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
}

#[test]
fn healthy_source_read_failure_preserves_original_flight_until_exact_cancellation() {
    let mut run = selected_run("mapped-page-flight-source-cancel", 141);
    let (pass, page, finish) = admitted(&mut run, "Q");
    stage_sources(&mut run, pass, false);
    let request = MutationPageRequest::new(page).with_pass(pass);
    run.host.test_set_mutation_transition_limit(1);
    run.host
        .test_arm_mutation_before_execute_fault(|store, storage| {
            support::bump_home_revision(storage.clone(), store, 119)
        });
    run.host.test_fail_next_mutation_staging_page_exact_read();
    expect_read_failure(run.host.stage_mutation_page(
        &run.fixture.store,
        request.clone(),
        Box::new([]),
    ));
    let custody = run.host.test_mutation_in_flight_custody().unwrap();
    assert_retained(&run, &request, custody, false);
    assert_eq!(run.fixture.store.health().state(), HomeHealthState::Healthy);
    assert!(run.host.test_fail_mutation_staging_page_exact_read());
    let executions = execution_counter(&mut run);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        run.host.execute_mutation(
            &run.fixture.store,
            MutationCommitRequest::new(finish.key(), MutationIdentity::ROOT),
            &cancellation
        ),
        Err(ComposerHostError::MutationStaging(
            syndic_storage::DraftMutationStagingErrorV1::Read(_)
        ))
    ));
    assert_retained(&run, &request, custody, false);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    run.host.test_set_mutation_transition_limit(4096);
    cancel(&mut run, finish.key());
    assert_eq!(request.page().payload_owner_count(), 1);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    let head = run
        .fixture
        .storage
        .draft_mutation_staging_head(
            &run.fixture.store,
            staging_identity(run.binding, finish.key().operation().get()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 0);
}

#[test]
fn failed_home_read_retains_page_flight_without_claiming_manual_retry_or_cancellation() {
    let mut run = selected_run("mapped-page-flight-failed-home", 151);
    let (pass, page, finish) = admitted(&mut run, "é");
    stage_sources(&mut run, pass, false);
    let request = MutationPageRequest::new(page).with_pass(pass);
    let faults = run.fixture.faults.clone();
    run.host
        .test_arm_mutation_before_execute_fault(move |_, _| {
            faults.fail_next(FaultPoint::BeforeReadConfirmation)
        });
    expect_read_failure(run.host.stage_mutation_page(
        &run.fixture.store,
        request.clone(),
        Box::new([]),
    ));
    let custody = run.host.test_mutation_in_flight_custody().unwrap();
    assert_retained(&run, &request, custody, false);
    assert_eq!(run.fixture.store.health().state(), HomeHealthState::Failed);
    let executions = execution_counter(&mut run);
    assert!(
        run.host
            .stage_mutation_page(&run.fixture.store, request.clone(), Box::new([]))
            .is_err()
    );
    assert_retained(&run, &request, custody, false);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(
        run.host
            .execute_mutation(
                &run.fixture.store,
                MutationCommitRequest::new(finish.key(), MutationIdentity::ROOT),
                &cancellation
            )
            .is_err()
    );
    assert_retained(&run, &request, custody, false);
    assert_eq!(run.fixture.store.health().state(), HomeHealthState::Failed);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
}
