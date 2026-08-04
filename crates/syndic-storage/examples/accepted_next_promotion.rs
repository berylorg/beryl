use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{SyndicItemId, SyndicTurnId};
use syndic_storage::{
    ACCEPTED_NEXT_PAGE_MAX_BYTES, ACCEPTED_NEXT_PAGE_MAX_RECORDS, PromoteAcceptedInput,
    SyndicStorage, SyndicTimestamp,
};

#[allow(dead_code)]
fn prepare_earliest_promotion(
    home: &HomeStore,
    syndic: SyndicStorage,
    successor_turn_id: SyndicTurnId,
    successor_item_id: SyndicItemId,
    promoted_at: SyndicTimestamp,
) -> Result<Option<PromoteAcceptedInput>, Box<dyn std::error::Error>> {
    let source_revision = syndic.revision(home)?;
    let limits =
        CursorReadLimits::new(ACCEPTED_NEXT_PAGE_MAX_RECORDS, ACCEPTED_NEXT_PAGE_MAX_BYTES)?;
    let mut source_cursor = None;
    let mut unavailable_thread = None;

    loop {
        let source_page =
            syndic.accepted_next_source_page(home, source_revision, source_cursor, limits)?;
        let next_source_cursor = source_page.next_cursor();
        for source in source_page.records().iter().copied() {
            if unavailable_thread == Some(source.thread_id()) {
                continue;
            }
            let mut candidate_cursor = None;
            loop {
                let candidate_page =
                    syndic.accepted_next_candidate_page(home, source, candidate_cursor, limits)?;
                let next_candidate_cursor = candidate_page.next_cursor();
                if let Some(candidate) = candidate_page.into_candidate() {
                    return Ok(Some(PromoteAcceptedInput::new(
                        candidate,
                        successor_turn_id,
                        successor_item_id,
                        promoted_at,
                    )));
                }
                let Some(cursor) = next_candidate_cursor else {
                    break;
                };
                candidate_cursor = Some(cursor);
            }
            unavailable_thread = Some(source.thread_id());
        }
        let Some(cursor) = next_source_cursor else {
            return Ok(None);
        };
        source_cursor = Some(cursor);
    }
}

fn main() {}
