use super::*;
use beryl_app::main_window::{
    MainWindowShellAbandonmentOutcome as Abandonment,
    MainWindowShellAbandonmentPreparationOutcome as Preparation,
    MainWindowShellAbandonmentReconciliationOutcome as Reconciliation,
    MainWindowShellPreparationFailure,
};
use beryl_home_store::test_faults::FaultPoint;

#[test]
fn foreign_home_matching_identifiers_reject_preparation_and_cleanup_without_losing_custody() {
    let mut source = ShellFixture::new(41);
    let foreign = ShellFixture::new(41);
    let result = MainWindowShellPrepared::prepare(
        &source.process,
        &foreign.service,
        MainWindowShellPreparationRequest::new(
            source.acquisition.take().unwrap(),
            foreign.composer.clone(),
            configurator(),
            foreign.marker_seals.clone(),
            submission_source(),
            foreign.appearance.clone(),
        ),
    );
    let Err(MainWindowShellPreparationFailure::Composer { unpublished, .. }) = result else {
        panic!("foreign binding must reject")
    };
    assert_eq!(source.process.main_window_occupancy(), 1);
    let Preparation::NotCommitted { unpublished, .. } =
        unpublished.prepare_abandonment(&foreign.service, CommandCancellation::new())
    else {
        panic!("foreign cleanup must retain custody")
    };
    assert_eq!(source.process.main_window_occupancy(), 1);
    assert_eq!(
        foreign
            .state
            .session()
            .minimal_bootstrap(&foreign.store)
            .unwrap()
            .unwrap()
            .windows()
            .len(),
        1
    );
    let Preparation::ExactAcquired { abandonment } =
        unpublished.prepare_abandonment(&source.service, CommandCancellation::new())
    else {
        panic!("own cleanup must prepare")
    };
    assert!(matches!(
        abandonment.abandon(&source.service, CommandCancellation::new()),
        Abandonment::Committed { .. }
    ));
    assert_eq!(source.process.main_window_occupancy(), 0);
    assert_eq!(
        foreign
            .state
            .session()
            .minimal_bootstrap(&foreign.store)
            .unwrap()
            .unwrap()
            .windows()
            .len(),
        1
    );
}

#[test]
fn not_committed_indeterminate_and_pending_retain_exact_reservation_until_terminal() {
    let mut fixture = ShellFixture::new(51);
    let unpublished = fixture.prepare().into_unpublished();
    let cancel = CommandCancellation::new();
    cancel.cancel();
    let Preparation::NotCommitted { unpublished, .. } =
        unpublished.prepare_abandonment(&fixture.service, cancel)
    else {
        panic!("cancelled preparation retains exact custody")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    let Preparation::ExactAcquired { abandonment } =
        unpublished.prepare_abandonment(&fixture.service, CommandCancellation::new())
    else {
        panic!("prepare cleanup")
    };
    let cancel = CommandCancellation::new();
    cancel.cancel();
    let Abandonment::NotCommitted { abandonment, .. } =
        abandonment.abandon(&fixture.service, cancel)
    else {
        panic!("cancelled command retains exact custody")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    let block = fixture.faults.block_next(FaultPoint::BeforeCommit);
    let service = fixture.service.clone();
    let worker = support::worker(move || abandonment.abandon(&service, CommandCancellation::new()));
    assert!(block.wait_until_reached(std::time::Duration::from_secs(10)));
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    block.release();
    let Abandonment::Indeterminate { reconciliation, .. } = worker.join().unwrap() else {
        panic!("acknowledgement loss retains reconciliation")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    let foreign = ShellFixture::new(61);
    let Reconciliation::Pending { reconciliation, .. } = reconciliation.reconcile(&foreign.store)
    else {
        panic!("foreign reconciliation must remain pending")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert!(matches!(
        reconciliation.reconcile(&fixture.store),
        Reconciliation::ExactAbandoned { .. }
    ));
    assert_eq!(fixture.process.main_window_occupancy(), 0);
}
