use beryl_home_store::{DomainRegistrationError, DomainValidationError, HomeRecoveryError};
use syndic_storage::{SyndicStorage, test_faults::FixtureBatch};

use super::{TestHome, commit, open};

pub fn exercise_case(
    name: &str,
    expected: &str,
    seed: impl Fn() -> FixtureBatch,
    corrupt: impl Fn() -> FixtureBatch,
) {
    let registration_home = TestHome::new(&format!("{name}-registration"));
    let mut store = open(registration_home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, seed());
    store.validate_registered_domains().unwrap();
    commit(&store, storage, corrupt());
    store.close().unwrap();

    let mut reopened = open(registration_home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("corrupted Syndic domain registered successfully"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected exact semantic registration rejection, got {other:?}"),
    }
    reopened.close().unwrap();

    let recovery_home = TestHome::new(&format!("{name}-recovery"));
    let mut store = open(recovery_home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, seed());
    store.validate_registered_domains().unwrap();
    commit(&store, storage, corrupt());
    assert_rejected(store.validate_registered_domains().unwrap_err(), expected);
    match store.recover_same_home().unwrap_err() {
        HomeRecoveryError::DomainValidation(error) => assert_rejected(error, expected),
        other => panic!("expected exact semantic recovery rejection, got {other:?}"),
    }
    store.close().unwrap();
}

fn assert_rejected(error: DomainValidationError, expected: &str) {
    match error {
        DomainValidationError::Rejected { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected exact semantic validation rejection, got {other:?}"),
    }
}
