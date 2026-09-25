use super::*;
use crate::activity_service::preparation::ActivityPreparationError;
use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion};
use support::TestHome;

#[test]
fn candidate_preparation_preserves_provenance_and_admits_only_after_publication() {
    let directory = TestHome::new("activity-service-preparation");
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut candidate = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let mut runtime = owner(candidate.home_id());
    let source = runtime
        .activity_read_source(candidate.home_id(), candidate.generation())
        .unwrap();
    let prepared =
        PreparedActivityService::prepare(&mut candidate, storage, source, limits(1, 1, 1)).unwrap();
    assert!(matches!(
        prepared
            .service_for_test()
            .prepare_collection(id(30), support::exact_cas::execution_binding().runtime_id()),
        Err(ActivityReadError::Unavailable)
    ));
    let home = candidate.publish().unwrap();
    let service = prepared.admit_published(&home).unwrap();
    assert!(matches!(
        service.prepare_collection(id(30), support::exact_cas::execution_binding().runtime_id()),
        Err(ActivityReadError::Runtime(
            RuntimeActivityReadError::RuntimeUnavailable
        ))
    ));
    service.retire();
    assert!(service.shared.state.lock().unwrap().resources.is_none());
    drop(service);
    assert!(runtime.shutdown());
    home.close().unwrap();
}

#[test]
fn foreign_candidate_sources_and_storage_are_rejected_and_abandonment_releases_custody() {
    let foreign = Fixture::new(true);
    let directory = TestHome::new("activity-service-abandonment");
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut candidate = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let wrong_source = foreign
        .owner
        .activity_read_source(
            foreign.home.home_id(),
            foreign.home.health().generation().unwrap(),
        )
        .unwrap();
    assert!(matches!(
        PreparedActivityService::prepare(
            &mut candidate,
            storage.clone(),
            wrong_source,
            limits(1, 1, 1)
        ),
        Err(ActivityPreparationError::Identity)
    ));
    let mut runtime = owner(candidate.home_id());
    let source = runtime
        .activity_read_source(candidate.home_id(), candidate.generation())
        .unwrap();
    assert!(
        PreparedActivityService::prepare(
            &mut candidate,
            foreign.storage.clone(),
            source.clone(),
            limits(1, 1, 1)
        )
        .is_err()
    );
    let prepared =
        PreparedActivityService::prepare(&mut candidate, storage, source, limits(1, 1, 1)).unwrap();
    drop(prepared);
    assert!(runtime.shutdown());
    candidate.close().unwrap();
    let reopened = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    reopened.close().unwrap();
}
