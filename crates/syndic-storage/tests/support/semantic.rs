use beryl_home_store::{DomainRegistrationError, DomainValidationError};
use syndic_storage::{SyndicStorage, test_faults::FixtureBatch};

use super::{TestHome, commit, open, populated::seed_populated};

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
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(&store, storage, corrupt());
    store.close().unwrap();

    let mut routine_reopened = open(registration_home.path());
    SyndicStorage::register(&mut routine_reopened).unwrap();
    routine_reopened.close().unwrap();

    let mut reopened = open(registration_home.path());
    let error = match SyndicStorage::register_with_schema_validation(&mut reopened) {
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
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(&store, storage, corrupt());
    assert_rejected(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .validation_error(),
        expected,
    );
    let candidate = store.recover_same_home().unwrap();
    SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let recovered = candidate.publish();
    SyndicStorage::reacquire(&recovered).unwrap();
    recovered.close().unwrap();
}

pub fn exercise_seeded_populated_case(
    name: &str,
    expected: &str,
    corrupt: impl Fn(&beryl_home_store::HomeStore, SyndicStorage) -> FixtureBatch,
) {
    let registration_home = TestHome::new(&format!("{name}-registration"));
    let mut store = open(registration_home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(&store, storage, corrupt(&store, storage));
    store.close().unwrap();

    let mut routine_reopened = open(registration_home.path());
    SyndicStorage::register(&mut routine_reopened).unwrap();
    routine_reopened.close().unwrap();

    let mut reopened = open(registration_home.path());
    let error = match SyndicStorage::register_with_schema_validation(&mut reopened) {
        Ok(_) => panic!("corrupted Syndic domain registered successfully"),
        Err(error) => error,
    };
    assert_registration_rejected(error, expected);
    reopened.close().unwrap();

    let recovery_home = TestHome::new(&format!("{name}-recovery"));
    let mut store = open(recovery_home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(&store, storage, corrupt(&store, storage));
    assert_rejected(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .validation_error(),
        expected,
    );
    let candidate = store.recover_same_home().unwrap();
    SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let recovered = candidate.publish();
    SyndicStorage::reacquire(&recovered).unwrap();
    recovered.close().unwrap();
}

fn assert_registration_rejected(error: DomainRegistrationError, expected: &str) {
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected exact semantic registration rejection, got {other:?}"),
    }
}

fn assert_rejected(error: &DomainValidationError, expected: &str) {
    match error {
        DomainValidationError::Rejected { domain, source } => {
            assert_eq!(*domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected exact semantic validation rejection, got {other:?}"),
    }
}
