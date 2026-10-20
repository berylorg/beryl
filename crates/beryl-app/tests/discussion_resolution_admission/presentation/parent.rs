use super::*;
use beryl_model::{SyndicItemId, SyndicTurnId};

#[test]
fn parent_acceptance_terminal_gap_and_archive_have_distinct_coherent_presentations() {
    for status in [
        TurnEndStatus::complete(),
        TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
    ] {
        let fixture = Fixture::new();
        let reader = reader(&fixture);
        let job = admitted(
            fixture
                .admit("parent-status", 210, "Original parent resolution")
                .unwrap(),
        );
        let command = fixture.service.live_home_command().unwrap();
        support::discussion_handoff::complete_resolving_turn(command.home(), &fixture.syndic);
        drop(command);
        assert!(matches!(
            fixture
                .settlement
                .prepare(job, CommandCancellation::new())
                .unwrap()
                .unwrap()
                .execute(),
            DiscussionSettlementOutcome::Committed {
                result: DiscussionSettlementResult::ReadyForParent,
                later_failure: None,
                ..
            }
        ));
        let command = fixture.service.live_home_command().unwrap();
        support::converge_and_release_terminal_history(
            command.home(),
            fixture.syndic.clone(),
            id(30),
            support::populated::source_turn(),
        );
        drop(command);
        let request = DiscussionParentInputRequest {
            turn_id: SyndicTurnId::from_bytes([240; 16]),
            item_id: SyndicItemId::from_bytes([241; 16]),
            admitted_at: support::timestamp(500),
        };
        assert!(matches!(
            fixture
                .settlement
                .prepare_parent_input(job, request, CommandCancellation::new())
                .unwrap()
                .unwrap()
                .execute(),
            DiscussionSettlementOutcome::Committed {
                result: DiscussionSettlementResult::StartingParent(_),
                later_failure: None,
                ..
            }
        ));
        let before = reader.observe(id(36)).unwrap();
        assert_eq!(before.status(), Some(DiscussionStatus::ResolutionPending));
        let command = fixture.service.live_home_command().unwrap();
        let source = support::exact_cas::establish_turn(
            command.home(),
            fixture.syndic.clone(),
            id(30),
            request.turn_id,
            support::timestamp(501),
        );
        drop(command);
        let accepted = reader.observe(id(36)).unwrap();
        assert_eq!(accepted.status(), Some(DiscussionStatus::HandingOff));
        assert_eq!(accepted.composer(), DiscussionComposerContribution::Inert);
        assert!(matches!(
            fixture
                .settlement
                .prepare_parent_execution(job, support::timestamp(600), CommandCancellation::new())
                .unwrap()
                .unwrap()
                .execute(),
            DiscussionSettlementOutcome::Committed {
                result: DiscussionSettlementResult::ParentActive(_),
                later_failure: None,
                ..
            }
        ));
        let command = fixture.service.live_home_command().unwrap();
        support::exact_cas::admit_event(
            command.home(),
            fixture.syndic.clone(),
            id(30),
            request.turn_id,
            &source,
            SourceEventPayload::TurnActivated,
            support::timestamp(502),
        );
        support::exact_cas::correlate_user_item(
            command.home(),
            fixture.syndic.clone(),
            id(30),
            request.turn_id,
            request.item_id,
            &source,
            support::timestamp(503),
        );
        support::exact_cas::admit_event(
            command.home(),
            fixture.syndic.clone(),
            id(30),
            request.turn_id,
            &source,
            SourceEventPayload::TurnEnded(status.clone()),
            support::timestamp(504),
        );
        drop(command);
        let terminal_gap = reader.observe(id(36)).unwrap();
        assert_eq!(
            terminal_gap.status(),
            Some(DiscussionStatus::ResolutionPending)
        );
        assert_eq!(
            terminal_gap.composer(),
            DiscussionComposerContribution::Inert
        );
        assert!(matches!(
            fixture
                .settlement
                .prepare_parent_execution(job, support::timestamp(601), CommandCancellation::new())
                .unwrap()
                .unwrap()
                .execute(),
            DiscussionSettlementOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let settled = reader.observe(id(36)).unwrap();
        if status == TurnEndStatus::complete() {
            assert_eq!(settled.status(), Some(DiscussionStatus::Archived));
            assert_eq!(settled.composer(), DiscussionComposerContribution::ReadOnly);
        } else {
            assert_eq!(settled.status(), Some(DiscussionStatus::HandoffFailed));
            assert_eq!(settled.composer(), DiscussionComposerContribution::Writable);
            assert!(settled.detail().is_some());
        }
        assert!(settled.retry().is_none());
        assert_eq!(settled.attempt().unwrap().job_id(), job);
        fixture.close();
    }
}
