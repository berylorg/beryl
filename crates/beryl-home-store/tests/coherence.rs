mod support;

use std::panic::{AssertUnwindSafe, catch_unwind};

use beryl_home_store::{HomeCoherenceError, HomeDomainRequirements};
use tempfile::tempdir;

#[test]
fn coherent_election_returns_only_the_callback_result_without_mutating_storage() {
    let directory = tempdir().unwrap();
    let store = support::open_home(directory.path())
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap()
        .publish()
        .unwrap();
    let generation = store.health().generation().unwrap();
    let revision = store.home_revision().unwrap();
    let mut calls = 0;
    assert_eq!(
        store.try_elect_coherent(generation, || {
            calls += 1;
            42
        }),
        Ok(42)
    );
    assert_eq!(calls, 1);
    assert_eq!(store.home_revision().unwrap(), revision);
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

#[test]
fn poisoned_authority_refuses_election_without_invoking_the_callback() {
    let directory = tempdir().unwrap();
    let store = support::open_home(directory.path())
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap()
        .publish()
        .unwrap();
    let generation = store.health().generation().unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = store.try_elect_coherent(generation, || panic!("election panic"));
        }))
        .is_err()
    );
    assert_eq!(
        store.try_elect_coherent(generation, || panic!("poisoned election ran")),
        Err(HomeCoherenceError::Unavailable)
    );
}

#[cfg(feature = "test-faults")]
#[path = "coherence/custody.rs"]
mod custody;

#[cfg(feature = "test-faults")]
#[path = "coherence/concurrency.rs"]
mod concurrency;
