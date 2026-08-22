use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use syndic_storage::test_faults::PersistedProviderNarrativeCorruption;

use super::{restart::*, *};

#[test]
fn provable_partial_narrative_corruption_is_rejected_by_scrub() {
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
        match store.execute_current(storage.current_begin_provider_frame_build(&prepared)) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean provider-frame build begin, got {outcome:?}"),
        }

        let mut first = None;
        let stale_home_revision = store.home_revision().unwrap();
        let stopped = stage_provider_frame(
            &prepared,
            prepared.initial_build().clone(),
            &mut |batch: &ProviderFrameStageBatch| {
                if first.is_none() {
                    first = Some(batch.clone());
                    store.execute_current(storage.current_stage_provider_frame_batch(batch.clone()))
                } else {
                    let mut command = HomeCommand::new(stale_home_revision);
                    command
                        .add(storage.stage_provider_frame_batch(
                            storage.revision(&store).unwrap(),
                            batch.clone(),
                        ))
                        .unwrap();
                    store.execute(command)
                }
            },
        )
        .unwrap();
        match stopped {
            ProviderFrameStageOutcome::NotCommitted { .. } => {}
            ProviderFrameStageOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                reconciliation.install();
                panic!(
                    "expected partial staging interruption to be definitive, got indeterminate {failure:?}"
                )
            }
            ProviderFrameStageOutcome::Committed {
                receipt,
                later_failure,
                ..
            } => panic!(
                "expected partial staging interruption, got committed outcome with receipt {receipt:?} and later failure {later_failure:?}"
            ),
            ProviderFrameStageOutcome::Unchanged { value } => {
                panic!("expected partial staging interruption, got unchanged build {value:?}")
            }
        }
        let batch = first.unwrap();
        let build = batch.next_build();
        let span = batch.narrative_spans()[0];
        assert_eq!(build.lifecycle(), ProviderItemBuildLifecycle::Staging);
        assert!(span.source_end() <= build.staged_encoded_bytes());
        let command = storage
            .current_corrupt_staged_provider_narrative(build, span, corruption)
            .unwrap();
        match store.execute_current(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected committed staged-provider corruption, got {outcome:?}"),
        }
        store.close().unwrap();

        let mut reopened = HomeStore::open(HomeOpenOptions::new(
            home.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        SyndicStorage::register(&mut reopened).unwrap();
        let error = reopened
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "unexpected scrub rejection for {corruption:?}: {error}"
        );
        reopened.close().unwrap();
    }
}
