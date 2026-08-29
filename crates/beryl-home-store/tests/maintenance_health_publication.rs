#![cfg(feature = "test-faults")]

mod support;

use std::num::NonZeroU64;

use beryl_home_store::{
    CommitReceiptError, HomeCommand, HomeHealthState, HomeStore, ReadError, SidecarByteLimit,
    SidecarError, SidecarNamespace, SidecarStage,
};
use tempfile::tempdir;

use support::{AlphaDomain, PutBytes, committed};

fn sidecar_limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1_024).unwrap())
}

fn assert_failed(store: &HomeStore) {
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(matches!(
        store.home_revision(),
        Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Failed
    ));
}

#[test]
fn receipt_revision_rejects_an_unobserved_fjall_maintenance_terminal() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"durable".to_vec()),
        ))
        .unwrap();
    let receipt = committed(store.execute(command));

    store.inject_retained_maintenance_terminal();

    assert!(matches!(
        store.receipt_domain_revision(&receipt, &alpha),
        Err(CommitReceiptError::StorageHealth { .. })
    ));
    assert_failed(&store);
}

#[test]
fn sidecar_admission_rejects_an_unobserved_fjall_maintenance_terminal() {
    let directory = tempdir().unwrap();
    let store = support::open_home(directory.path());

    store.inject_retained_maintenance_terminal();

    assert!(matches!(
        store.admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"not publishable",
            sidecar_limit(),
        ),
        Err(SidecarError::Storage {
            stage: SidecarStage::ConfirmHealth,
            ..
        })
    ));
    assert_failed(&store);
}

#[test]
fn sidecar_verification_rejects_an_unobserved_fjall_maintenance_terminal() {
    let directory = tempdir().unwrap();
    let store = support::open_home(directory.path());
    let admitted = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"already durable",
            sidecar_limit(),
        )
        .unwrap();
    let address = admitted.address().clone();

    store.inject_retained_maintenance_terminal();

    assert!(matches!(
        store.verify_sidecar(&address, sidecar_limit()),
        Err(SidecarError::Storage {
            stage: SidecarStage::ConfirmHealth,
            ..
        })
    ));
    assert_failed(&store);
}
