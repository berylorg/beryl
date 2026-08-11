#[path = "provider_broker_checked_user/support.rs"]
mod support;

use beryl_backend::{
    NormalTurnTerminalStatus, OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause,
    UserMessageEchoLifecycle,
};
#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_model::{CasItemId, CasTurnId};
use syndic_storage::{
    BindingState, CasItemSource, CasTurnSource, InputGateState, ProviderFrameOrdinalV1,
    ProviderItemLifecycle, SourceEventPayload, SourceEventSequence, TurnEndStatus,
    TurnIncompleteReason, TurnLifecycle, TurnTerminalOutcome,
};
use support::*;

#[test]
fn checked_user_acknowledgements_follow_exact_activation_and_same_item_publication() {
    let mut fixture = CheckedUserFixture::new(171);
    let cas_item_id = CasItemId::new("checked-user-item-171").unwrap();
    assert!(
        fixture
            .storage
            .active_cas_turn(&fixture.home, fixture.snapshot_id, point_limit())
            .unwrap()
            .is_none()
    );

    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());

    let active = fixture
        .storage
        .active_cas_turn(&fixture.home, fixture.snapshot_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(active.thread_id(), fixture.thread_id);
    assert_eq!(active.turn_id(), fixture.turn_id);
    assert_eq!(active.cas_thread_id(), &fixture.cas_thread_id);
    assert_eq!(active.cas_turn_id(), &fixture.cas_turn_id);
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        2
    );
    let activation = fixture.source_event(1);
    assert_eq!(
        activation.source(),
        Some(&CasTurnSource::new(
            fixture.cas_thread_id.clone(),
            fixture.cas_turn_id.clone(),
        ))
    );
    assert!(matches!(
        activation.payload(),
        SourceEventPayload::TurnActivated
    ));
    let started_event = fixture.source_event(2);
    let SourceEventPayload::ItemFrame {
        item_id: started_item_id,
        frame: started_reference,
    } = started_event.payload()
    else {
        panic!("checked-user start did not publish an item frame")
    };
    assert_eq!(*started_item_id, fixture.item_id);
    let started_reference = started_reference.as_ref().clone();
    assert_eq!(
        started_reference.frame().ordinal(),
        ProviderFrameOrdinalV1::FIRST
    );
    let started_item = fixture.canonical_item();
    assert_eq!(started_item.id(), fixture.item_id);
    assert_eq!(
        started_item.presentation_content(),
        Some(fixture.submitted_content)
    );
    assert_eq!(
        started_item.provider_lifecycle(),
        ProviderItemLifecycle::Started
    );
    assert_eq!(
        started_item.source_event(),
        Some(SourceEventSequence::new(2).unwrap())
    );
    assert_eq!(
        started_item.cas_source(),
        Some(&CasItemSource::new(
            CasTurnSource::new(fixture.cas_thread_id.clone(), fixture.cas_turn_id.clone(),),
            cas_item_id.clone(),
        ))
    );
    assert_eq!(started_item.provider(), Some(&started_reference));
    assert_user_message_frame(
        &read_provider_frame(&fixture.home, fixture.storage, &started_reference),
        ProviderFrameOrdinalV1::FIRST,
        UserMessageEchoLifecycle::Started,
        &cas_item_id,
        fixture.submitted_content,
    );

    fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id.clone());

    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        3
    );
    let completed_event = fixture.source_event(3);
    let SourceEventPayload::ItemFrame {
        item_id: completed_item_id,
        frame: completed_reference,
    } = completed_event.payload()
    else {
        panic!("checked-user completion did not publish an item frame")
    };
    assert_eq!(*completed_item_id, fixture.item_id);
    let completed_reference = completed_reference.as_ref().clone();
    assert_eq!(
        completed_reference.frame().ordinal(),
        ProviderFrameOrdinalV1::new(2).unwrap()
    );
    let completed_item = fixture.canonical_item();
    assert_eq!(completed_item.id(), fixture.item_id);
    assert_eq!(
        completed_item.presentation_content(),
        Some(fixture.submitted_content)
    );
    assert_eq!(
        completed_item.provider_lifecycle(),
        ProviderItemLifecycle::Completed
    );
    assert_eq!(
        completed_item.source_event(),
        Some(SourceEventSequence::new(3).unwrap())
    );
    assert_eq!(completed_item.provider(), Some(&completed_reference));
    assert_user_message_frame(
        &read_provider_frame(&fixture.home, fixture.storage, &completed_reference),
        ProviderFrameOrdinalV1::new(2).unwrap(),
        UserMessageEchoLifecycle::Completed,
        &cas_item_id,
        fixture.submitted_content,
    );

    assert_eq!(
        completed_reference.content().id(),
        started_reference.content().id()
    );
    assert_eq!(
        completed_reference.frame().encoded_start(),
        started_reference.content().summary().encoded_bytes()
    );
    assert_eq!(
        started_reference
            .content()
            .revision()
            .checked_next()
            .unwrap(),
        completed_reference.content().revision()
    );
    assert_eq!(
        completed_reference.stream_state().started_at(),
        started_reference.stream_state().started_at()
    );
    assert!(completed_reference.stream_state().is_complete());
    assert!(
        fixture
            .storage
            .provider_item_build(&fixture.home, fixture.item_id, point_limit())
            .unwrap()
            .is_none()
    );

    let home_revision_before = fixture.home.home_revision().unwrap();
    let syndic_revision_before = fixture.storage.revision(&fixture.home).unwrap();
    let turn_before = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    let item_before = fixture.canonical_item();
    fixture
        .broker
        .as_ref()
        .unwrap()
        .prove_response_activation(&fixture.registration.proof(), &fixture.cas_turn_id)
        .unwrap();
    assert_eq!(fixture.home.home_revision().unwrap(), home_revision_before);
    assert_eq!(
        fixture.storage.revision(&fixture.home).unwrap(),
        syndic_revision_before
    );
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap(),
        turn_before
    );
    assert_eq!(fixture.canonical_item(), item_before);

    fixture.close();
}

#[test]
fn checked_user_publication_barrier_holds_one_real_permit_and_releases_it() {
    let mut fixture = CheckedUserFixture::new(174);
    let cas_item_id = CasItemId::new("checked-user-item-174").unwrap();

    fixture.submit_checked_while_publication_paused(
        UserMessageEchoLifecycle::Started,
        cas_item_id,
        |fixture| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
            let blocked = loop {
                let snapshot = fixture.broker_snapshot();
                if snapshot.in_flight().current() == 1 {
                    break snapshot;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "checked-user submission did not enter its capacity-one acknowledgement path"
                );
                std::thread::yield_now();
            };
            assert_eq!(blocked.in_flight().high_water(), 1);
            assert_eq!(blocked.submitted(), 1);
            assert_eq!(blocked.acked(), 0);
            assert_eq!(blocked.checked_user_publications().activity().current(), 1);
            assert_eq!(
                blocked.checked_user_publications().activity().high_water(),
                1
            );
            assert_eq!(blocked.checked_user_publications().publications(), 1);
        },
    );

    let released = fixture.broker_snapshot();
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.submitted(), 1);
    assert_eq!(released.acked(), 1);
    assert_eq!(
        released.checked_user_publications().activity().current(),
        0
    );
    assert_eq!(
        released
            .checked_user_publications()
            .activity()
            .high_water(),
        1
    );
    assert_eq!(released.checked_user_publications().publications(), 1);

    fixture.close();
}

#[test]
fn mismatched_completed_item_closes_the_exact_target_without_advancing_lifecycle() {
    let mut fixture = CheckedUserFixture::new(172);
    let cas_item_id = CasItemId::new("checked-user-item-172").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id);
    let started_item = fixture.canonical_item();
    let syndic_revision_before = fixture.storage.revision(&fixture.home).unwrap();

    fixture.submit_checked(
        UserMessageEchoLifecycle::Completed,
        CasItemId::new("wrong-checked-user-item-172").unwrap(),
    );

    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::SourcePublicationFailed
        )
    );
    assert_eq!(fixture.canonical_item(), started_item);
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        2
    );
    assert!(
        fixture
            .storage
            .source_event(
                &fixture.home,
                fixture.turn_id,
                SourceEventSequence::new(3).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture.storage.revision(&fixture.home).unwrap(),
        syndic_revision_before
    );

    fixture.close();
}

include!("provider_broker_checked_user/terminal.rs");

#[test]
fn normal_terminal_before_turn_start_closes_only_the_exact_target() {
    let mut fixture = CheckedUserFixture::before_turn_start(178);
    fixture.submit_terminal(NormalTurnTerminalStatus::Completed);

    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::EventBeforeTurnStart
        )
    );
    let state = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.source_event_count(), 0);
    assert!(state.end_status().is_none());

    fixture.close();
}

#[test]
fn normal_terminal_route_mismatch_does_not_publish_a_terminal_event() {
    let mut fixture = CheckedUserFixture::new(179);
    let cas_item_id = CasItemId::new("checked-user-item-179").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id);
    let rejected = fixture
        .try_submit_terminal_for_route(
            NormalTurnTerminalStatus::Completed,
            fixture.cas_thread_id.clone(),
            CasTurnId::new("wrong-terminal-turn-179").unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        rejected.cause(),
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl)
    );
    assert!(
        fixture
            .try_submit_terminal_for_route(
                NormalTurnTerminalStatus::Completed,
                fixture.cas_thread_id.clone(),
                fixture.cas_turn_id.clone(),
            )
            .is_err()
    );

    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::ConflictingTurnIdentity
        )
    );
    let state = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Active);
    assert_eq!(state.source_event_count(), 2);
    assert!(state.end_status().is_none());
    assert!(
        fixture
            .storage
            .source_event(
                &fixture.home,
                fixture.turn_id,
                SourceEventSequence::new(3).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );

    fixture.close();
}
