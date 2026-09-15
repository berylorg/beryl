use syndic_storage::{
    CompactionProviderEvent, CompactionThreadStatus, StopAdmissionIneligibility, StopAdmissionRead,
    SyndicStorage, TurnEndStatus, TurnTerminalOutcome,
};

use super::compaction_support::point_limit;

#[test]
fn candidate_provider_stop_preserves_live_and_finalizing_authority() {
    for finalizing in [false, true] {
        let (fixture, operation, _) = super::provider_stop::admit_provider_stop(
            "candidate-provider-stop",
            201 + u8::from(finalizing),
        );
        if finalizing {
            fixture.publish_provider(
                operation,
                CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
                24,
            );
            fixture.publish_provider(
                operation,
                CompactionProviderEvent::Terminal(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
                25,
            );
        }
        let expected = fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit())
            .unwrap();
        if finalizing {
            assert!(matches!(
                expected,
                StopAdmissionRead::Ineligible(StopAdmissionIneligibility::Compacting { .. })
            ));
        } else {
            assert!(matches!(expected, StopAdmissionRead::Stopping(_)));
        }
        fixture.store.close().unwrap();
        let mut candidate = crate::support::open(fixture.home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let mut publication = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap();
        assert_eq!(
            storage
                .stop_admission_read_candidate(
                    &publication.recovery_access().unwrap(),
                    fixture.thread,
                    point_limit()
                )
                .unwrap(),
            expected
        );
        let access = publication.recovery_access().unwrap();
        let source = storage
            .delivery_recovery_startup_page_candidate(
                &access,
                None,
                beryl_home_store::CursorReadLimits::new(1, 65_536).unwrap(),
            )
            .unwrap()
            .records()[0]
            .clone();
        let candidate_case = storage
            .classify_delivery_recovery_candidate(&access, &source, point_limit())
            .unwrap();
        assert!(
            matches!(
                &candidate_case,
                syndic_storage::DeliveryRecoveryCase::DeferredCompaction { .. } if finalizing
            ) || matches!(
                &candidate_case,
                syndic_storage::DeliveryRecoveryCase::Stopping(_) if !finalizing
            )
        );
        let store = publication.publish().unwrap();
        assert_eq!(
            storage
                .classify_delivery_recovery(&store, &source, point_limit())
                .unwrap(),
            candidate_case
        );
        assert_eq!(
            storage
                .stop_admission_read(&store, fixture.thread, point_limit())
                .unwrap(),
            expected
        );
        store.close().unwrap();
    }
}
