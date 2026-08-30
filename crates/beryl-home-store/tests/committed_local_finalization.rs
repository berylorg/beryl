#![cfg(feature = "test-faults")]

mod support;

use std::{cell::Cell, io};

use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    CommittedLocalFinalizationError, DomainAttachmentAccessError, DomainHandle, HomeCommand,
    HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use support::{AlphaDomain, BetaDomain, PutBytes, committed};

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn command<D: beryl_home_store::StorageDomain>(
    store: &HomeStore,
    domain: &DomainHandle<D>,
    key: u64,
) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<D>::new(key, vec![key as u8]),
        ))
        .unwrap();
    command
}

fn committed_with_local(outcome: CommandOutcome) -> (CommitReceipt, CommittedLocalFinalization) {
    match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(CommandError::Persistence { .. }),
            local_finalization: Some(local_finalization),
        } => (receipt, local_finalization),
        other => panic!("expected committed local-finalization outcome, got {other:?}"),
    }
}

#[test]
fn normal_commit_has_no_local_finalization() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    assert!(matches!(
        store.execute(command(&store, &alpha, 1)),
        CommandOutcome::Committed {
            later_failure: None,
            local_finalization: None,
            ..
        }
    ));
}

#[test]
fn after_persist_capability_finalizes_the_live_attachment_without_reopening_health() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let attachment = alpha.attachment_capability();

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let (receipt, local_finalization) =
        committed_with_local(store.execute(command(&store, &alpha, 2)));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(store.receipt_domain_revision(&receipt, &alpha).is_err());
    assert!(matches!(
        store.with_domain_attachment(&attachment, |_| ()),
        Err(DomainAttachmentAccessError::HealthGate(_))
    ));

    let invoked = Cell::new(0);
    let result = store
        .with_committed_local_finalization(local_finalization, &receipt, &alpha, |_| {
            invoked.set(invoked.get() + 1);
            17
        })
        .unwrap();
    assert_eq!(result, 17);
    assert_eq!(invoked.get(), 1);
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(store.receipt_domain_revision(&receipt, &alpha).is_err());
}

#[test]
fn substituted_receipt_is_rejected_without_invoking_the_callback() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let earlier_receipt = committed(store.execute(command(&store, &alpha, 3)));

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let (_receipt, local_finalization) =
        committed_with_local(store.execute(command(&store, &alpha, 4)));
    let invoked = Cell::new(false);
    assert!(matches!(
        store.with_committed_local_finalization(
            local_finalization,
            &earlier_receipt,
            &alpha,
            |_| invoked.set(true),
        ),
        Err(CommittedLocalFinalizationError::ReceiptMismatch)
    ));
    assert!(!invoked.get());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

#[test]
fn wrong_domain_is_rejected_without_invoking_the_callback() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let (receipt, local_finalization) =
        committed_with_local(store.execute(command(&store, &alpha, 5)));
    let invoked = Cell::new(false);
    assert!(matches!(
        store.with_committed_local_finalization(local_finalization, &receipt, &beta, |_| invoked
            .set(true),),
        Err(CommittedLocalFinalizationError::WrongDomain { .. })
    ));
    assert!(!invoked.get());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

#[test]
fn foreign_store_and_recovered_generation_are_rejected_without_callback() {
    let first_directory = tempdir().unwrap();
    let first_faults = FaultController::new();
    let mut first = open(first_directory.path(), first_faults.clone());
    let first_alpha = first.register_domain::<AlphaDomain>().unwrap();
    first_faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let (first_receipt, first_local) =
        committed_with_local(first.execute(command(&first, &first_alpha, 6)));

    let second_directory = tempdir().unwrap();
    let mut second = open(second_directory.path(), FaultController::new());
    let second_alpha = second.register_domain::<AlphaDomain>().unwrap();
    let foreign_invoked = Cell::new(false);
    assert!(matches!(
        second.with_committed_local_finalization(
            first_local,
            &first_receipt,
            &second_alpha,
            |_| foreign_invoked.set(true),
        ),
        Err(CommittedLocalFinalizationError::StaleOrForeign)
    ));
    assert!(!foreign_invoked.get());
    assert_eq!(first.health().state(), HomeHealthState::Failed);
    assert_eq!(second.health().state(), HomeHealthState::Healthy);

    let third_directory = tempdir().unwrap();
    let third_faults = FaultController::new();
    let mut third = open(third_directory.path(), third_faults.clone());
    let third_alpha = third.register_domain::<AlphaDomain>().unwrap();
    third_faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let (third_receipt, third_local) =
        committed_with_local(third.execute(command(&third, &third_alpha, 7)));
    let candidate = third.recover_same_home().unwrap();
    let recovered_alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let recovered = candidate.publish();
    let stale_invoked = Cell::new(false);
    assert!(matches!(
        recovered.with_committed_local_finalization(
            third_local,
            &third_receipt,
            &recovered_alpha,
            |_| stale_invoked.set(true),
        ),
        Err(CommittedLocalFinalizationError::StaleOrForeign)
    ));
    assert!(!stale_invoked.get());
    assert_eq!(recovered.health().state(), HomeHealthState::Healthy);
}

#[test]
fn finalization_uses_generation_identity_when_writer_identity_has_diverged() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let _ = committed_with_local(store.execute(command(&store, &alpha, 8)));
    faults.fail_next(FaultPoint::AfterReopen);
    let failed = store.recover_same_home().unwrap_err().into_store();
    let candidate = failed.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let recovered = candidate.publish();

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let (receipt, local_finalization) =
        committed_with_local(recovered.execute(command(&recovered, &alpha, 9)));
    let invoked = Cell::new(false);
    recovered
        .with_committed_local_finalization(local_finalization, &receipt, &alpha, |_| {
            invoked.set(true)
        })
        .unwrap();
    assert!(invoked.get());
}

#[test]
fn multi_domain_commit_has_no_local_finalization() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let mut command = command(&store, &alpha, 8);
    command
        .add(beta.contribution(
            store.domain_revision(&beta).unwrap(),
            PutBytes::<BetaDomain>::new(8, vec![8]),
        ))
        .unwrap();

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: Some(CommandError::Persistence { .. }),
            local_finalization: None,
            ..
        }
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}
