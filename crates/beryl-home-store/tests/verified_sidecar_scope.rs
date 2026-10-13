#![cfg(feature = "test-faults")]

#[path = "verified_sidecar_scope/fixture.rs"]
mod fixture;
mod support;

use std::io;

use beryl_home_store::{
    HomeHealthState, HomeStore, SidecarError, SidecarStage, VerifiedSidecar,
    test_faults::{FaultController, FaultPoint},
};
use fixture::{PublishedState, limit, open, owned_directory, seed, verified};

fn assert_gate_error(store: &HomeStore, token: &VerifiedSidecar, state: HomeHealthState) {
    let error = store
        .verified_sidecar_matches_current_scope(token)
        .unwrap_err();
    let SidecarError::HealthGate(gate) = &error else {
        panic!("unexpected scope qualification error: {error:?}");
    };
    assert_eq!(gate.state(), state);
    assert_eq!(Some(gate.generation()), store.health().generation());
}

#[test]
fn current_scope_reuses_verified_authority_without_changing_published_state() {
    let path = owned_directory();
    let store = open(&path, &FaultController::new());
    let address = seed(&store);
    let token = verified(&store, &address);
    let published = PublishedState::capture(&store);
    let reference = store.service_reference();
    for _ in 0..4 {
        assert!(
            store
                .verified_sidecar_matches_current_scope(&token)
                .unwrap()
        );
        assert!(
            reference
                .verified_sidecar_matches_current_scope(&token)
                .unwrap()
        );
    }
    assert_eq!(PublishedState::capture(&store), published);
    drop(reference);
    store.close().unwrap();
    let reopened = open(&path, &FaultController::new());
    assert_eq!(PublishedState::capture(&reopened), published);
    reopened.close().unwrap();
}

#[test]
fn foreign_homes_with_equal_addresses_and_generations_do_not_share_authority() {
    let path = owned_directory();
    let foreign_path = owned_directory();
    let store = open(&path, &FaultController::new());
    let foreign = open(&foreign_path, &FaultController::new());
    let address = seed(&store);
    let foreign_address = seed(&foreign);
    let token = verified(&store, &address);
    let foreign_token = verified(&foreign, &foreign_address);
    assert_eq!(token.address(), foreign_token.address());
    assert_eq!(token.generation(), foreign_token.generation());
    let published = PublishedState::capture(&store);
    let foreign_published = PublishedState::capture(&foreign);
    assert!(
        !foreign
            .verified_sidecar_matches_current_scope(&token)
            .unwrap()
    );
    assert!(
        !store
            .verified_sidecar_matches_current_scope(&foreign_token)
            .unwrap()
    );
    assert_eq!(PublishedState::capture(&store), published);
    assert_eq!(PublishedState::capture(&foreign), foreign_published);
    store.close().unwrap();
    foreign.close().unwrap();
}

#[test]
fn a_fresh_open_cannot_qualify_a_token_from_the_same_path_and_generation_number() {
    let path = owned_directory();
    let store = open(&path, &FaultController::new());
    let address = seed(&store);
    let token = verified(&store, &address);
    let published = PublishedState::capture(&store);
    let home_id = store.home_id();
    store.close().unwrap();
    let reopened = open(&path, &FaultController::new());
    let current_token = verified(&reopened, &address);
    assert_eq!(reopened.home_id(), home_id);
    assert_eq!(token.path(), current_token.path());
    assert_eq!(token.address(), current_token.address());
    assert_eq!(token.generation(), current_token.generation());
    assert!(
        !reopened
            .verified_sidecar_matches_current_scope(&token)
            .unwrap()
    );
    assert!(
        reopened
            .verified_sidecar_matches_current_scope(&current_token)
            .unwrap()
    );
    assert_eq!(PublishedState::capture(&reopened), published);
    reopened.close().unwrap();
}

#[test]
fn recovery_rejects_old_tokens_and_retired_facades_preserve_gate_provenance() {
    let path = owned_directory();
    let faults = FaultController::new();
    let store = open(&path, &faults);
    let address = seed(&store);
    let token = verified(&store, &address);
    let published = PublishedState::capture(&store);
    let retired = store.service_reference();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    assert_gate_error(&store, &token, HomeHealthState::Failed);
    let candidate = store.recover_same_home().unwrap();
    let unpublished = candidate.service_reference();
    assert_gate_error(&unpublished, &token, HomeHealthState::Reopening);
    let recovered = candidate.publish().unwrap();
    assert!(
        !recovered
            .verified_sidecar_matches_current_scope(&token)
            .unwrap()
    );
    assert_gate_error(&retired, &token, HomeHealthState::Healthy);
    let current_token = verified(&recovered, &address);
    assert!(
        recovered
            .verified_sidecar_matches_current_scope(&current_token)
            .unwrap()
    );
    assert_gate_error(&retired, &current_token, HomeHealthState::Healthy);
    assert_eq!(PublishedState::capture(&recovered), published);
    drop(retired);
    drop(unpublished);
    recovered.close().unwrap();
    let reopened = open(&path, &FaultController::new());
    assert_eq!(PublishedState::capture(&reopened), published);
    reopened.close().unwrap();
}

#[test]
fn closed_facades_report_the_health_gate_instead_of_a_scope_mismatch() {
    let path = owned_directory();
    let store = open(&path, &FaultController::new());
    let address = seed(&store);
    let token = verified(&store, &address);
    let published = PublishedState::capture(&store);
    let closed = store.service_reference();
    store.close().unwrap();
    assert_gate_error(&closed, &token, HomeHealthState::Failed);
    drop(closed);
    let reopened = open(&path, &FaultController::new());
    assert_eq!(PublishedState::capture(&reopened), published);
    reopened.close().unwrap();
}

#[test]
fn scope_qualification_leaves_final_file_verification_fault_unconsumed() {
    let path = owned_directory();
    let faults = FaultController::new();
    let store = open(&path, &faults);
    let address = seed(&store);
    let token = verified(&store, &address);
    let published = PublishedState::capture(&store);
    faults.fail_next_with_kind(
        FaultPoint::BeforeSidecarVerification,
        io::ErrorKind::PermissionDenied,
    );
    assert!(
        store
            .verified_sidecar_matches_current_scope(&token)
            .unwrap()
    );
    assert_eq!(PublishedState::capture(&store), published);
    let error = store.verify_sidecar(&address, limit()).unwrap_err();
    let SidecarError::Storage { stage, source } = error else {
        panic!("scope qualification consumed the verification fault: {error:?}");
    };
    assert_eq!(stage, SidecarStage::OpenFinal);
    assert_eq!(
        source.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_gate_error(&store, &token, HomeHealthState::Failed);
    let recovered = store.recover_same_home().unwrap().publish().unwrap();
    assert_eq!(PublishedState::capture(&recovered), published);
    recovered.close().unwrap();
}
