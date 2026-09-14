use beryl_home_store::CursorReadLimits;
use syndic_storage::{DeliveryRecoveryCase, StopAdmissionIneligibility, StopAdmissionRead};

use super::compaction_support::{CompactionFixture, point_limit};

#[cfg(feature = "test-faults")]
#[path = "finalization_classification/corruption.rs"]
mod corruption;

#[test]
fn terminal_outcomes_remain_deferred_with_and_without_prior_stop() {
    use syndic_storage::{
        CompactionProviderEvent, CompactionRecoveryCase, CompactionThreadStatus, TurnEndStatus,
        TurnTerminalOutcome,
    };
    for (index, outcome) in [
        TurnTerminalOutcome::Complete,
        TurnTerminalOutcome::Interrupted,
        TurnTerminalOutcome::Failed,
    ]
    .into_iter()
    .enumerate()
    {
        for prior_stop in [false, true] {
            let seed = 173 + index as u8 * 2 + u8::from(prior_stop);
            let (fixture, id, stop_id) = if prior_stop {
                let (fixture, id, stop) = super::provider_stop::admit_provider_stop(
                    "terminal-compaction-prior-stop",
                    seed,
                );
                (fixture, id, Some(stop))
            } else {
                let fixture = CompactionFixture::new("terminal-compaction-outcome", seed);
                let id = fixture.admit(seed.wrapping_add(20), 10);
                fixture.claim(id);
                fixture.publish_provider(
                    id,
                    CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
                    20,
                );
                fixture.publish_provider(
                    id,
                    CompactionProviderEvent::TurnStarted(
                        beryl_model::CasTurnId::new(format!("finalization-{seed}")).unwrap(),
                    ),
                    21,
                );
                (fixture, id, None)
            };
            let stale_source = source(&fixture);
            if outcome == TurnTerminalOutcome::Complete {
                let item_id = beryl_model::SyndicItemId::from_bytes([seed; 16]);
                for (offset, lifecycle) in [
                    syndic_storage::CompactionMarkerLifecycle::Started,
                    syndic_storage::CompactionMarkerLifecycle::Completed,
                ]
                .into_iter()
                .enumerate()
                {
                    fixture.publish_provider(
                        id,
                        CompactionProviderEvent::Marker { item_id, lifecycle },
                        22 + offset as u64,
                    );
                }
            }
            fixture.publish_provider(
                id,
                CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
                24,
            );
            fixture.publish_provider(
                id,
                CompactionProviderEvent::Terminal(TurnEndStatus::new(outcome, None).unwrap()),
                25,
            );
            assert_deferred(&fixture, id);
            if let Some(stop_id) = stop_id {
                assert!(matches!(
                    fixture
                        .storage
                        .stop_operation(&fixture.store, stop_id, point_limit())
                        .unwrap()
                        .unwrap()
                        .state(),
                    syndic_storage::StopOperationState::MatchingTerminal(_)
                ));
                assert!(matches!(
                    fixture.storage.classify_delivery_recovery(
                        &fixture.store,
                        &stale_source,
                        point_limit()
                    ),
                    Err(syndic_storage::DeliveryRecoveryClassificationError::SourceDrift)
                ));
            }
            let recovery = fixture
                .storage
                .compaction_recovery_read(&fixture.store, id, point_limit())
                .unwrap()
                .unwrap();
            match outcome {
                TurnTerminalOutcome::Interrupted => assert!(matches!(
                    recovery,
                    CompactionRecoveryCase::FinalizeInterruptedWithIdleEvidence(_)
                )),
                TurnTerminalOutcome::Complete => assert!(matches!(
                    recovery,
                    CompactionRecoveryCase::FinalizeSuccess(_)
                )),
                TurnTerminalOutcome::Failed => assert!(matches!(
                    recovery,
                    CompactionRecoveryCase::FinalizeFailure(_)
                )),
                _ => unreachable!("test matrix uses exact provider outcomes"),
            }
        }
    }
}

fn source(fixture: &CompactionFixture) -> syndic_storage::DeliveryRecoverySource {
    fixture
        .storage
        .delivery_recovery_startup_page(
            &fixture.store,
            None,
            CursorReadLimits::new(64, 64 * 1024).unwrap(),
        )
        .unwrap()
        .records()
        .iter()
        .find(|source| source.thread_id() == fixture.thread)
        .unwrap()
        .clone()
}

#[test]
fn terminal_compaction_reads_preserve_point_limits() {
    let fixture = CompactionFixture::new("terminal-compaction-point-limit", 180);
    let id = fixture.admit(200, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    let source = source(&fixture);
    let limit = syndic_storage::SyndicPointReadLimit::new(1).unwrap();
    assert!(matches!(
        fixture
            .storage
            .classify_delivery_recovery(&fixture.store, &source, limit),
        Err(syndic_storage::DeliveryRecoveryClassificationError::Read(_))
    ));
    assert!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, limit)
            .is_err()
    );
    assert_deferred(&fixture, id);
}

#[test]
fn terminal_compaction_remains_deferred_until_settlement() {
    let fixture = CompactionFixture::new("terminal-compaction-classification", 167);
    let id = fixture.admit(187, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    assert_deferred(&fixture, id);
}

pub(super) fn assert_deferred(
    fixture: &CompactionFixture,
    id: syndic_storage::CompactionOperationId,
) {
    let operation = fixture.operation(id);
    let gate = fixture.gate();
    let page = fixture
        .storage
        .delivery_recovery_startup_page(
            &fixture.store,
            None,
            CursorReadLimits::new(64, 64 * 1024).unwrap(),
        )
        .unwrap();
    let source = page
        .records()
        .iter()
        .find(|source| source.thread_id() == fixture.thread)
        .unwrap();
    assert!(matches!(
        fixture.storage.classify_delivery_recovery(&fixture.store, source, point_limit()).unwrap(),
        DeliveryRecoveryCase::DeferredCompaction { thread_id, turn_id }
            if thread_id == fixture.thread && turn_id == id.provider_turn_id()
    ));
    assert!(matches!(
        fixture.storage.stop_admission_read(&fixture.store, fixture.thread, point_limit()).unwrap(),
        StopAdmissionRead::Ineligible(StopAdmissionIneligibility::Compacting { turn_id, current_gate_revision })
            if turn_id == id.provider_turn_id() && current_gate_revision == gate.revision()
    ));
    assert_eq!(fixture.operation(id), operation);
    assert_eq!(fixture.gate(), gate);
}
