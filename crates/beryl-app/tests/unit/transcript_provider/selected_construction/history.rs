use super::*;
use crate::support::exact_cas::{
    admit_event, admit_started_then_completed_item, converge_and_release_terminal_history,
    correlate_user_item, establish_turn, submit_current_draft,
};
use beryl_model::CasItemId;

pub(super) fn create_inherited_discussion(
    fixture: &Fixture,
    turns: u8,
) -> (SyndicTurnId, SyndicItemId) {
    let parent = fixture.request(1).thread_id;
    let mut last = None;
    for index in 0..turns {
        let user = SyndicItemId::from_bytes([40 + index * 3; 16]);
        let assistant = SyndicItemId::from_bytes([41 + index * 3; 16]);
        let turn = submit_current_draft(
            &fixture.home,
            fixture.storage.clone(),
            parent,
            SyndicDraftId::from_bytes([42 + index * 3; 16]),
            user,
            "Discuss this response",
            SyndicTimestamp::from_unix_millis(100 + index as u64 * 10),
        );
        let at = SyndicTimestamp::from_unix_millis(101 + index as u64 * 10);
        let source = establish_turn(&fixture.home, fixture.storage.clone(), parent, turn, at);
        admit_event(
            &fixture.home,
            fixture.storage.clone(),
            parent,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            at,
        );
        correlate_user_item(
            &fixture.home,
            fixture.storage.clone(),
            parent,
            turn,
            user,
            &source,
            at,
        );
        let message = || {
            ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
                text: ProviderTextV1::inline(CONTEXT),
                phase: Some(ProviderMessagePhaseV1::FinalAnswer),
                memory_citation: None,
            })
        };
        admit_started_then_completed_item(
            &fixture.home,
            fixture.storage.clone(),
            parent,
            turn,
            assistant,
            &source,
            CasItemId::new(format!("selected-context-{index}")).unwrap(),
            message(),
            message(),
            at,
            at,
        );
        admit_event(
            &fixture.home,
            fixture.storage.clone(),
            parent,
            turn,
            &source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
            at,
        );
        converge_and_release_terminal_history(&fixture.home, fixture.storage.clone(), parent, turn);
        last = Some((turn, assistant));
    }
    let (turn, item) = last.unwrap();
    let thread = fixture
        .storage
        .thread(&fixture.home, parent, point())
        .unwrap()
        .unwrap();
    let head = fixture
        .storage
        .transcript_view_head(&fixture.home, parent, point())
        .unwrap()
        .unwrap();
    let entries = fixture
        .storage
        .transcript_entries(
            &fixture.home,
            parent,
            head.generation(),
            None,
            beryl_home_store::CursorReadLimits::new(64, 65_536).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == item)
        .unwrap();
    let source = fixture
        .storage
        .prepare_discussion_source(
            &fixture.home,
            DiscussionContextSource::new(
                parent,
                turn,
                item,
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(0, CONTEXT.len() as u64).unwrap(),
            ),
            thread.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
            DiscussionContextText::new(CONTEXT).unwrap(),
        )
        .unwrap();
    let prepared = fixture
        .storage
        .prepare_discussion_creation(
            &fixture.home,
            source,
            CreateDiscussion::new(
                child(),
                SyndicDraftId::from_bytes([181; 16]),
                SyndicTimestamp::from_unix_millis(500),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        )
        .unwrap();
    execute(&fixture.home, prepared.contribution());
    (turn, item)
}

pub(super) fn start(fixture: &Fixture) {
    let thread = fixture
        .storage
        .thread(&fixture.home, child(), point())
        .unwrap()
        .unwrap();
    execute(
        &fixture.home,
        fixture.storage.start_transcript_build(
            fixture.storage.revision(&fixture.home).unwrap(),
            StartTranscriptBuild::new(child(), thread.revision(), head(fixture).revision()),
        ),
    );
}

pub(super) fn advance(fixture: &Fixture) {
    let build = build(fixture).unwrap();
    execute(
        &fixture.home,
        fixture.storage.advance_transcript_build(
            fixture.storage.revision(&fixture.home).unwrap(),
            AdvanceTranscriptBuild::new(child(), build.generation(), build.revision()),
        ),
    );
}
