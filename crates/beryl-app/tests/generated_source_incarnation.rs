#![cfg(all(feature = "test-faults", target_os = "windows"))]

#[path = "generated_output_sources/support.rs"]
mod support;

#[path = "generated_source_incarnation/runtime.rs"]
mod runtime;

#[path = "generated_source_incarnation/coordinator.rs"]
mod coordinator;

use beryl_backend::{GeneratedOutputError, GeneratedOutputTarget, ThreadLoadOptions};
use beryl_model::{CasItemId, CasTurnId};
use std::{fs, io::Read, time::Duration};
use support::Fixture;

fn target() -> GeneratedOutputTarget {
    GeneratedOutputTarget::new(
        [62; 16],
        CasTurnId::new("incarnation-turn").unwrap(),
        CasItemId::new("incarnation-item").unwrap(),
    )
}

#[test]
fn original_loaded_nonce_survives_file_work_and_another_original_owner_has_distinct_provenance() {
    let mut original = Fixture::new();
    let historical = original.loaded.generated_source_incarnation().unwrap();
    let replacement = Fixture::new();
    assert_eq!(original.loaded.thread_id(), replacement.loaded.thread_id());
    assert_ne!(Some(historical), replacement.loaded.generated_source_incarnation());
    assert_eq!(replacement.session.authorize_generated_output(
        &replacement.release, &original.loaded, target(),
    ).err(), Some(GeneratedOutputError::LostSourceAuthority));
    fs::write(original.path(), b"retained original").unwrap();
    let source = original.open();
    let mut bytes = Vec::new();
    source.cursor().read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"retained original");
    assert!(source.close().is_ok());
    assert_eq!(original.loaded.generated_source_incarnation(), Some(historical));
    assert!(original.session.authorize_generated_output(
        &original.release, &original.loaded, target(),
    ).is_ok());
}

#[test]
fn superseding_the_actual_loaded_owner_revokes_every_prior_nonce_and_file_permit() {
    let mut fixture = Fixture::new();
    let old = fixture.loaded.clone();
    let incarnation = old.generated_source_incarnation().unwrap();
    let permit = fixture.session.authorize_generated_output(&fixture.release, &old, target()).unwrap();
    let loaded = fixture.session.resume_thread(
        old.thread_id(), &ThreadLoadOptions::for_root(fixture.directory.path().join("root")), Duration::from_secs(10),
    ).unwrap();
    assert_eq!(loaded.thread_id(), old.thread_id());
    assert_ne!(loaded.generated_source_incarnation(), Some(incarnation));
    assert_eq!(old.generated_source_incarnation(), None);
    assert_eq!(fixture.session.authorize_generated_output(&fixture.release, &old, target()).err(),
        Some(GeneratedOutputError::LostSourceAuthority));
    fs::write(fixture.path(), b"superseded").unwrap();
    let path = beryl_model::RuntimeNativePath::from_admitted(
        beryl_model::RuntimeMode::Host, beryl_model::PathFlavor::Windows,
        fixture.path().to_str().unwrap(),
    ).unwrap();
    assert_eq!(fixture.server.open_generated_output(
        permit, &path, beryl_backend::GeneratedReaderAllowance::new(2).unwrap(),
        fixture.cancelled.clone(),
    ).err().unwrap().error(), GeneratedOutputError::LostSourceAuthority);
    assert!(fixture.session.authorize_generated_output(&fixture.release, &loaded, target()).is_ok());
}
