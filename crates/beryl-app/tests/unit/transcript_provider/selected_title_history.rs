use super::*;
use crate::support::exact_cas::{
    admit_event, converge_and_release_terminal_history, correlate_user_item, establish_turn,
    submit_current_draft,
};
use syndic_storage::{SourceEventPayload, TurnEndStatus};

const AUTHORED_TITLE: &str = "Actual canonical user title";

fn seed_user_history(fixture: &Fixture) -> beryl_model::SyndicTurnId {
    let thread = fixture.request(1).thread_id;
    let item = beryl_model::SyndicItemId::from_bytes([41; 16]);
    let turn = submit_current_draft(
        &fixture.home,
        fixture.storage.clone(),
        thread,
        SyndicDraftId::from_bytes([42; 16]),
        item,
        AUTHORED_TITLE,
        SyndicTimestamp::from_unix_millis(101),
    );
    let source = establish_turn(
        &fixture.home,
        fixture.storage.clone(),
        thread,
        turn,
        SyndicTimestamp::from_unix_millis(102),
    );
    admit_event(
        &fixture.home,
        fixture.storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        SyndicTimestamp::from_unix_millis(103),
    );
    correlate_user_item(
        &fixture.home,
        fixture.storage.clone(),
        thread,
        turn,
        item,
        &source,
        SyndicTimestamp::from_unix_millis(104),
    );
    admit_event(
        &fixture.home,
        fixture.storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        SyndicTimestamp::from_unix_millis(105),
    );
    converge_and_release_terminal_history(&fixture.home, fixture.storage.clone(), thread, turn);
    turn
}

#[test]
fn selected_title_uses_authentic_committed_user_history_and_keeps_its_publication_fence() {
    let fixture = Fixture::new();
    seed_user_history(&fixture);
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    assert_eq!(
        prepared.resolved_title().source(),
        beryl_state::CatalogTitleSource::HistoryDerived
    );
    assert_eq!(prepared.resolved_title().text(), Some(AUTHORED_TITLE));
    fixture
        .reader
        .publish_if_current(prepared, &request, &cancel, |seed| {
            let Some(crate::syndic_transcript::TranscriptProviderResponseKind::ViewPage(page)) =
                seed.view_page_response
            else {
                panic!("actual history view is missing")
            };
            assert_eq!(page.records.len(), 1);
        })
        .unwrap();
}

#[test]
fn generated_selected_title_requires_actual_original_source_and_supersedes_old_completion() {
    let fixture = Fixture::new();
    let turn = seed_user_history(&fixture);
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let old = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    assert_eq!(old.resolved_title().text(), Some(AUTHORED_TITLE));
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let thread = fixture
        .storage
        .thread(&fixture.home, request.thread_id, limit)
        .unwrap()
        .unwrap();
    let item = fixture
        .storage
        .canonical_item(
            &fixture.home,
            beryl_model::SyndicItemId::from_bytes([41; 16]),
            limit,
        )
        .unwrap()
        .unwrap();
    let attributes = fixture
        .storage
        .thread_attributes(&fixture.home, request.thread_id, limit)
        .unwrap()
        .unwrap();
    let generated = syndic_storage::GeneratedThreadTitle::new(
        "Actual generated title",
        turn,
        item.presentation_content().unwrap(),
        thread.selected_path_digest(),
        thread.revision(),
        SyndicTimestamp::from_unix_millis(106),
    )
    .unwrap();
    execute(
        &fixture.home,
        fixture.storage.accept_generated_thread_title(
            fixture.storage.revision(&fixture.home).unwrap(),
            syndic_storage::AcceptGeneratedThreadTitle::new(
                request.thread_id,
                attributes.revision(),
                generated,
            ),
        ),
    );
    let called = AtomicBool::new(false);
    assert_eq!(
        fixture
            .reader
            .publish_if_current(old, &request, &cancel, |_| called
                .store(true, Ordering::Release)),
        Err(TranscriptAttachmentError::Stale)
    );
    assert!(!called.load(Ordering::Acquire));
    let request = fixture.request(2);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    assert_eq!(
        prepared.resolved_title().source(),
        beryl_state::CatalogTitleSource::Generated
    );
    assert_eq!(
        prepared.resolved_title().text(),
        Some("Actual generated title")
    );
    fixture
        .reader
        .publish_if_current(prepared, &request, &cancel, |_| ())
        .unwrap();
}
