use beryl_home_store::{
    CommandError, DomainRegistrationError, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use syndic_storage::test_faults::PersistedProviderNarrativeCorruption;

use super::{restart::*, *};

#[derive(Debug, thiserror::Error)]
enum PartialStageError {
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("stop after the first durable partial batch")]
    Stop,
}

#[test]
fn provable_partial_narrative_corruption_is_rejected_on_reopen() {
    for (corruption, expected) in [
        (
            PersistedProviderNarrativeCorruption::SourceDigest,
            "provider span source range digest disagrees",
        ),
        (
            PersistedProviderNarrativeCorruption::StoredKey,
            "provider staged narrative span frontier disagrees",
        ),
        (
            PersistedProviderNarrativeCorruption::ResultingChainDigest,
            "provider staged narrative chain disagrees",
        ),
        (
            PersistedProviderNarrativeCorruption::StagedFrontier,
            "provider staged narrative ended before its build frontier",
        ),
    ] {
        let home = TestHome::new(&format!("corrupt-{corruption:?}"));
        let mut store = HomeStore::open(HomeOpenOptions::new(
            home.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        let prepared = narrative_ahead_prepared();
        store
            .execute_current(storage.current_begin_provider_frame_build(&prepared))
            .unwrap();

        let mut first = None;
        let stopped = stage_provider_frame(
            &prepared,
            prepared.initial_build().clone(),
            &mut |batch: &ProviderFrameStageBatch| -> Result<(), PartialStageError> {
                store.execute_current(storage.current_stage_provider_frame_batch(batch.clone()))?;
                first = Some(batch.clone());
                Err(PartialStageError::Stop)
            },
        )
        .unwrap_err();
        assert!(matches!(
            stopped,
            ProviderFrameStageError::Callback(PartialStageError::Stop)
        ));
        let batch = first.unwrap();
        let build = batch.next_build();
        let span = batch.narrative_spans()[0];
        assert_eq!(build.lifecycle(), ProviderItemBuildLifecycle::Staging);
        assert!(span.source_end() <= build.staged_encoded_bytes());
        let command = storage
            .current_corrupt_staged_provider_narrative(build, span, corruption)
            .unwrap();
        store.execute_current(command).unwrap();
        store.close().unwrap();

        let mut reopened = HomeStore::open(HomeOpenOptions::new(
            home.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let error = match SyndicStorage::register(&mut reopened) {
            Ok(_) => panic!("{corruption:?} reopened successfully"),
            Err(error) => error,
        };
        match error {
            DomainRegistrationError::Validation { domain, source } => {
                assert_eq!(domain, "syndic");
                assert_eq!(source.to_string(), expected);
            }
            other => panic!("expected provider validation rejection, got {other:?}"),
        }
        reopened.close().unwrap();
    }
}
