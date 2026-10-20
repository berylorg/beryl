use super::*;
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};

#[test]
fn stable_or_failed_revision_reads_cannot_qualify_a_nondispatched_source_drift() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = beryl_model::SyndicThreadId::from_bytes([217; 16]);
    let expected = storage.revision(&home).unwrap();
    let refusal = || ProjectionExecutionError::InjectionNotDispatched {
        thread_id: beryl_model::CasThreadId::new("unused-drift-target").unwrap(),
        source: Box::new(beryl_backend::ManagedBackendError::ThreadInjectionSource {
            method: "thread/inject_items".into(),
            source: ThreadInjectionSourceError::RevisionDrift,
            transport_bytes_written: false,
        }),
    };
    assert!(matches!(
        witness_source_revision(&home, &storage, thread, expected, refusal()),
        ProjectionExecutionError::InjectionNotDispatched { source, .. }
            if matches!(source.as_ref(), beryl_backend::ManagedBackendError::ThreadInjectionSource {
                source: ThreadInjectionSourceError::RevisionDrift,
                transport_bytes_written: false,
                ..
            })
    ));
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        witness_source_revision(&home, &storage, thread, expected, refusal()),
        ProjectionExecutionError::SyndicRead(syndic_storage::SyndicReadError::Read(_))
    ));
    home.close().unwrap();
}
