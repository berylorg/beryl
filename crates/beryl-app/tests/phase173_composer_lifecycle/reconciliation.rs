use super::{base, captured_autosave, composer, publication};
use beryl_app::composer_host::ComposerHostAutosaveAdvance;
use beryl_home_store::{CommandCancellation, test_faults::FaultPoint};
use beryl_state::BerylState;

#[test]
fn autosave_reconciliation_ignores_late_cancellation_and_converges_exact_new() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase173-autosave-late-cancel", 221);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 222, 223);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let timer = host.autosave_timer().unwrap();
    let cancellation = CommandCancellation::new();
    let ticket = captured_autosave(&mut host, &store, assets, &seals, timer, 224, &cancellation);
    host.test_arm_publication_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::ReconciliationPending
    );
    cancellation.cancel();
    assert_eq!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::Saved {
            dirty_successor: false
        }
    );
    assert_eq!(host.publication_custody_count(), 0);
    assert_eq!(host.lifecycle_diagnostics().joined_publications(), 0);
    assert!(host.autosave_timer().is_none());
}
