use super::*;
use beryl_home_store::{HomeCommand, WholeHomeScrubTrigger};
use beryl_model::RuntimeId;

fn event(
    f: &Fixture,
    source: Option<CasTurnSource>,
    payload: SourceEventPayload,
) -> LiveSourceEvent {
    let state = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    let gate = f
        .storage
        .input_gate(&f.store, id(30), limit())
        .unwrap()
        .unwrap();
    LiveSourceEvent::new(
        id(30),
        f.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        source,
        payload,
        timestamp(500),
    )
    .unwrap()
}

fn publish(
    f: &Fixture,
    event: LiveSourceEvent,
    qualification: ActivitySourceQualification,
) -> CommandOutcome {
    let mut command = HomeCommand::new(f.store.home_revision().unwrap());
    command
        .add(f.storage.admit_live_source_event(
            f.storage.revision(&f.store).unwrap(),
            event,
            qualification,
        ))
        .unwrap();
    f.store.execute(command)
}

fn current(token: &ActivityPeriodToken, turn: SyndicTurnId) -> ActivitySourceQualification {
    ActivitySourceQualification::Current {
        token: token.clone(),
        source: ActivityQuerySource::new(id(30), turn),
    }
}

#[test]
fn unenrolled_local_terminal_preserves_prior_activity_and_missing_owner_is_an_error() {
    let f = Fixture::new();
    let head = f.head();
    assert!(
        f.storage
            .activity_retirement_fingerprint(&f.store, ActivityQuerySource::new(id(30), f.turn))
            .unwrap()
            .is_none()
    );
    assert!(
        f.storage
            .activity_retirement_fingerprint(&f.store, ActivityQuerySource::new(id(249), f.turn))
            .is_err()
    );
    let terminal = event(
        &f,
        None,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
    );
    assert!(matches!(
        publish(&f, terminal, ActivitySourceQualification::Unenrolled),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(f.head(), head);
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn retired_fingerprint_cannot_stale_a_different_selected_period() {
    let f = Fixture::new();
    let previous_period = f.head().work_period();
    let token = f.enroll();
    let source = support::exact_cas::establish_turn(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        timestamp(101),
    );
    let head = f.head();
    let old =
        ActivityPeriodToken::for_fixture(f.store.home_id(), token.runtime_id(), previous_period);
    let fingerprint = ActivityRetirementFingerprint::from_enrollment(
        &old,
        ActivityQuerySource::new(id(30), f.turn),
    );
    assert!(matches!(
        publish(
            &f,
            event(&f, Some(source), SourceEventPayload::TurnActivated),
            ActivitySourceQualification::Retired(fingerprint)
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(f.head(), head);
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        1
    );
}

#[test]
fn current_producer_rejects_wrong_identity_without_canonical_progress() {
    let f = Fixture::new();
    let other = Fixture::new();
    let token = f.enroll();
    let foreign = other.enroll();
    let source = support::exact_cas::establish_turn(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        timestamp(101),
    );
    let activation = event(&f, Some(source), SourceEventPayload::TurnActivated);
    let before = f.store.home_revision().unwrap();
    let head = f.head();
    let state = f.storage.turn_state(&f.store, f.turn, limit()).unwrap();
    let wrong_runtime = ActivityPeriodToken::for_fixture(
        f.store.home_id(),
        RuntimeId::from_bytes([250; 16]),
        token.work_period(),
    );
    let wrong_period = ActivityPeriodToken::for_fixture(
        f.store.home_id(),
        token.runtime_id(),
        token.work_period().checked_next().unwrap(),
    );
    for qualification in [
        current(&foreign, f.turn),
        current(&wrong_runtime, f.turn),
        current(&wrong_period, f.turn),
        current(&token, SyndicTurnId::from_bytes([251; 16])),
        ActivitySourceQualification::Unenrolled,
        ActivitySourceQualification::Retired(ActivityRetirementFingerprint::from_enrollment(
            &token,
            ActivityQuerySource::new(id(30), SyndicTurnId::from_bytes([252; 16])),
        )),
    ] {
        assert!(matches!(
            publish(&f, activation.clone(), qualification),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(f.store.home_revision().unwrap(), before);
        assert_eq!(f.head(), head);
        assert_eq!(
            f.storage.turn_state(&f.store, f.turn, limit()).unwrap(),
            state
        );
    }
    assert!(matches!(
        publish(&f, activation, current(&token, f.turn)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(f.head().source_frontier(), head.source_frontier() + 1);
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn retired_terminal_stales_only_matching_head_and_survives_scrub() {
    let f = Fixture::new();
    let token = f.enroll();
    let source = support::exact_cas::establish_turn(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        timestamp(101),
    );
    assert!(matches!(
        publish(
            &f,
            event(&f, Some(source.clone()), SourceEventPayload::TurnActivated),
            current(&token, f.turn)
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let before = f.head();
    let retired = f
        .storage
        .activity_retirement_fingerprint(&f.store, ActivityQuerySource::new(id(30), f.turn))
        .unwrap()
        .unwrap();
    let terminal = event(
        &f,
        Some(source),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
    );
    assert!(matches!(
        publish(&f, terminal, ActivitySourceQualification::Retired(retired)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let head = f.head();
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
    assert!(!head.source_active());
    assert_eq!(head.source_frontier(), before.source_frontier());
    assert_eq!(head.source_count(), before.source_count());
    assert_eq!(head.completed_row_count(), before.completed_row_count());
    assert_eq!(head.running_row_count(), 0);
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn retired_item_completion_preserves_historical_row_through_fresh_enrollment() {
    let mut f = Fixture::new();
    let token = f.enroll();
    let source = support::exact_cas::establish_turn(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        timestamp(101),
    );
    support::exact_cas::admit_event(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(101),
    );
    support::exact_cas::correlate_user_item(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        SyndicItemId::from_bytes([221; 16]),
        &source,
        timestamp(102),
    );
    let item = SyndicItemId::from_bytes([245; 16]);
    let cas_item = beryl_model::CasItemId::new("retired-command").unwrap();
    support::exact_cas::admit_item_frame(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        item,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(103),
                item: retention::command(false),
            },
        ),
        timestamp(103),
    );
    let before = f.head();
    assert_eq!(before.running_row_count(), 1);
    let row = f
        .storage
        .activity_query_page(
            &f.store,
            &before,
            None,
            beryl_home_store::CursorReadLimits::new(32, 65_536).unwrap(),
        )
        .unwrap()
        .records()[0]
        .clone();
    let retired = ActivityRetirementFingerprint::from_enrollment(
        &token,
        ActivityQuerySource::new(id(30), f.turn),
    );
    let frame = support::exact_cas::stage_item_frame(
        &f.store,
        f.storage.clone(),
        f.turn,
        item,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(104),
                item: retention::command(true),
            },
        ),
    );
    assert!(matches!(
        publish(
            &f,
            event(
                &f,
                Some(source.clone()),
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(frame)
                }
            ),
            ActivitySourceQualification::Retired(retired.clone())
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(f.head().lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(f.head().source_frontier(), before.source_frontier());
    assert_eq!(f.head().logical_row_count(), 0);
    let rows = f
        .storage
        .fixture_activity_query_entry_count(
            &f.store,
            id(30),
            token.work_period(),
            beryl_home_store::CursorReadLimits::new(32, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(rows.0, 1);
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert!(matches!(
        publish(
            &f,
            event(
                &f,
                Some(source),
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(
                        TurnTerminalOutcome::Interrupted,
                        Some(TurnIncompleteReason::ItemAuditFailed)
                    )
                    .unwrap()
                )
            ),
            ActivitySourceQualification::Retired(retired)
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    support::converge_and_release_terminal_history(&f.store, f.storage.clone(), id(30), f.turn);
    f.turn = support::exact_cas::submit_current_draft(
        &f.store,
        f.storage.clone(),
        id(30),
        draft_id(246),
        SyndicItemId::from_bytes([247; 16]),
        "successor",
        timestamp(501),
    );
    let fresh = f.enroll();
    assert!(fresh.work_period() > token.work_period());
    assert_eq!(
        f.storage
            .fixture_activity_query_entry_count(
                &f.store,
                id(30),
                token.work_period(),
                beryl_home_store::CursorReadLimits::new(32, 65_536).unwrap()
            )
            .unwrap(),
        rows
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let corrupt = ActivityQueryEntryRecord::new(
        row.thread_id(),
        row.work_period(),
        row.order(),
        row.source().clone(),
        SourceEventSequence::new(row.source_event().get() + 1).unwrap(),
        row.provider_kind(),
        row.provider_lifecycle(),
        row.compact_fact().cloned(),
    )
    .unwrap();
    commit(
        &f.store,
        f.storage.clone(),
        batch([FixtureRecord::ActivityQueryEntry(corrupt)]),
    );
    let failure = f
        .store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        format!("{failure:?}").contains("immutable source frame"),
        "{failure:?}"
    );
}
