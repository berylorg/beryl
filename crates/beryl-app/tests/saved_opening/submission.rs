use beryl_app::composer_host::{ComposerHostSubmissionAdvance, ComposerHostSubmissionStage};
use beryl_model::{SyndicDraftId, SyndicItemId, SyndicTurnId};
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftHistoricalRootDirectionV1,
    DraftHistoricalRootSelectionIntentV1, FirstAcceptance, FirstAcceptanceKind, InputGateRecord,
    InputGateState,
    test_faults::{FixtureBatch, FixtureRecord},
};

use super::{
    base, composer,
    support::{Fixture, point_limit, submission_request},
};

#[test]
fn nonzero_reopened_submission_preserves_content_without_an_extra_selector_publication() {
    for queued in [false, true] {
        let mut fixture = Fixture::reopened("opening-submission", 110, "persisted content");
        if queued {
            let gate = fixture
                .storage
                .input_gate(&fixture.store, fixture.thread, point_limit())
                .unwrap()
                .unwrap();
            let gate = InputGateRecord::new(
                fixture.thread,
                gate.revision().checked_next().unwrap(),
                InputGateState::PendingTurn(SyndicTurnId::from_bytes([130; 16])),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap();
            let mut batch = FixtureBatch::new();
            batch.put(FixtureRecord::InputGate(gate)).unwrap();
            base::committed(base::execute(
                &fixture.store,
                fixture
                    .storage
                    .fixture_contribution(fixture.storage.revision(&fixture.store).unwrap(), batch),
            ));
        }
        let resident = fixture.binding();
        let durable = fixture.current();
        assert!(!fixture.host.is_dirty());
        assert!(fixture.host.autosave_timer().is_none());
        let ticket = fixture
            .host
            .begin_submission(submission_request(140))
            .unwrap();
        let acceptance = advance_to_accepting(&mut fixture, ticket, 150);
        assert_eq!(fixture.current(), durable);
        assert_eq!(acceptance.candidate(), resident.candidate());
        assert_eq!(acceptance.materialization().key().source(), resident.root());
        assert_eq!(fixture.binding(), resident);
        let expected = if queued {
            FirstAcceptanceKind::Accepted
        } else {
            FirstAcceptanceKind::Idle {
                user_item_id: SyndicItemId::from_bytes([141; 16]),
            }
        };
        assert_eq!(
            fixture.advance_submission(ticket, 150),
            ComposerHostSubmissionAdvance::ExactSuccess(expected)
        );
        assert!(fixture.host.binding().is_none());
        assert_eq!(fixture.host.publication_custody_count(), 0);
        assert_eq!(fixture.host.submission_diagnostics().retained_roots(), 0);
        assert_eq!(
            fixture
                .host
                .submission_diagnostics()
                .retained_materializations(),
            0
        );
        assert_eq!(
            fixture.current().draft().id(),
            SyndicDraftId::from_bytes([140; 16])
        );
        if queued {
            let accepted = fixture
                .storage
                .accepted_input(
                    &fixture.store,
                    resident.candidate().draft_id().accepted_input_id(),
                    point_limit(),
                )
                .unwrap()
                .unwrap();
            assert_eq!(accepted.content(), acceptance.materialization().content());
        } else {
            let item = fixture
                .storage
                .canonical_item(
                    &fixture.store,
                    SyndicItemId::from_bytes([141; 16]),
                    point_limit(),
                )
                .unwrap()
                .unwrap();
            assert_eq!(
                item.presentation_content().unwrap(),
                acceptance.materialization().content()
            );
        }
        let DraftEditorCandidateSessionReadOutcomeV1::Disposed(terminal) = fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                resident.candidate().draft_id(),
                resident.candidate().session_id(),
            )
            .unwrap()
        else {
            panic!("opening submission left no readable disposed session");
        };
        assert_eq!(terminal.newest_root(), resident.root());
        assert_eq!(terminal.newest_history(), durable.draft().history());
    }
}

#[test]
fn a_later_adoption_after_opening_capture_cannot_clear_the_newer_candidate() {
    let mut fixture = Fixture::reopened("submission-stale-capture", 160, "retained text");
    let resident = fixture.binding();
    let durable = fixture.current();
    let ticket = fixture
        .host
        .begin_submission(submission_request(180))
        .unwrap();
    let acceptance = advance_to_accepting(&mut fixture, ticket, 190);
    assert_eq!(acceptance.candidate(), resident.candidate());
    composer::direct_adopt(
        &fixture.store,
        fixture.storage.clone(),
        DraftHistoricalRootSelectionIntentV1::new(
            resident.candidate(),
            composer::operation_id(191),
            DraftHistoricalRootDirectionV1::Undo,
        ),
    );
    let later = fixture
        .storage
        .draft_editor_candidate_session(
            &fixture.store,
            resident.candidate().draft_id(),
            resident.candidate().session_id(),
        )
        .unwrap();
    let DraftEditorCandidateSessionReadOutcomeV1::Active(later) = later else {
        panic!("concurrent history adoption did not retain an active session");
    };
    assert!(later.newest_candidate_generation() > resident.candidate().candidate_generation());
    let revision = fixture.store.home_revision().unwrap();
    assert_eq!(
        fixture.advance_submission(ticket, 190),
        ComposerHostSubmissionAdvance::NotCommitted
    );
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(fixture.current(), durable);
    assert_eq!(
        fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                resident.candidate().draft_id(),
                resident.candidate().session_id()
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(later)
    );
    assert!(
        fixture
            .storage
            .canonical_item(
                &fixture.store,
                SyndicItemId::from_bytes([181; 16]),
                point_limit()
            )
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .storage
            .accepted_input(
                &fixture.store,
                resident.candidate().draft_id().accepted_input_id(),
                point_limit()
            )
            .unwrap()
            .is_none()
    );
}

fn advance_to_accepting(
    fixture: &mut Fixture,
    ticket: beryl_app::composer_host::ComposerHostSubmissionTicket,
    operation: u64,
) -> FirstAcceptance {
    let durable = fixture.current();
    for _ in 0..128 {
        assert_eq!(fixture.current(), durable);
        let outcome = fixture.advance_submission(ticket, operation);
        assert_eq!(fixture.current(), durable);
        if fixture.host.submission_diagnostics().stage()
            == Some(ComposerHostSubmissionStage::Accepting)
        {
            return fixture.host.test_submission_acceptance().unwrap();
        }
        assert!(
            matches!(outcome, ComposerHostSubmissionAdvance::Progress(_)),
            "opening submission did not reach acceptance: {outcome:?}"
        );
    }
    panic!("opening submission exceeded bounded capture/materialization work");
}
