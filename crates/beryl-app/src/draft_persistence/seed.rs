use beryl_home_store::{CursorReadLimits, HomeHealthState, HomeStore};
use beryl_model::SyndicThreadId;
use syndic_storage::{
    CONTENT_CHUNK_MAX_BYTES, ComposerContentAssembler, ComposerPayload, ContentChunkOrdinal,
    SyndicCurrentDraft, SyndicPointReadLimit, SyndicReadError, SyndicRecordError, SyndicStorage,
};
use thiserror::Error;

use super::{DraftPersistenceSeed, DraftPersistenceTime};

/// Reads one exact current draft and captures the same healthy generation around it.
pub fn read_draft_persistence_seed(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
    published_at: DraftPersistenceTime,
) -> Result<Option<DraftPersistenceSeed>, DraftSeedReadError> {
    let before = store.health();
    if before.state() != HomeHealthState::Healthy {
        return Err(DraftSeedReadError::HomeUnavailable);
    }
    let current = storage
        .current_draft(store, thread_id, limit)
        .map_err(DraftSeedReadError::CurrentDraft)?;
    let payload = current
        .as_ref()
        .map(|current| read_composer_payload(store, storage, current))
        .transpose()?;
    let current = match current {
        Some(current) => {
            let confirm = storage
                .current_draft(store, thread_id, limit)
                .map_err(DraftSeedReadError::CurrentDraft)?;
            if confirm.as_ref() != Some(&current) {
                return Err(DraftSeedReadError::GenerationChanged);
            }
            Some(current)
        }
        None => None,
    };
    let after = store.health();
    if before.generation() != after.generation() || after.state() != HomeHealthState::Healthy {
        return Err(DraftSeedReadError::GenerationChanged);
    }
    Ok(current.zip(payload).map(|(current, payload)| {
        DraftPersistenceSeed::new(
            store.home_id(),
            after.generation().expect("healthy homes have a generation"),
            current,
            payload,
            published_at,
        )
    }))
}

/// Why a current-draft preload or recovery seed could not be published.
#[derive(Debug, Error)]
pub enum DraftSeedReadError {
    #[error("Beryl home is not healthy")]
    HomeUnavailable,
    #[error("current draft read failed: {0}")]
    CurrentDraft(#[source] SyndicReadError),
    #[error("current draft content read failed: {0}")]
    ContentRead(#[source] SyndicReadError),
    #[error("current draft content is invalid: {0}")]
    Content(#[source] SyndicRecordError),
    #[error("Beryl-home generation changed during current-draft preload")]
    GenerationChanged,
}

fn read_composer_payload(
    store: &HomeStore,
    storage: &SyndicStorage,
    current: &SyndicCurrentDraft,
) -> Result<ComposerPayload, DraftSeedReadError> {
    let mut assembler = ComposerContentAssembler::new(current.draft().content())
        .map_err(DraftSeedReadError::Content)?;
    let mut after: Option<ContentChunkOrdinal> = None;
    loop {
        let limits = CursorReadLimits::new(16, 16 * (CONTENT_CHUNK_MAX_BYTES + 256))
            .expect("content page bounds are nonzero");
        let page = storage
            .content_chunks(store, current.draft().content().id(), after, limits)
            .map_err(DraftSeedReadError::ContentRead)?;
        if page.records().is_empty() && page.has_more() {
            return Err(DraftSeedReadError::ContentRead(SyndicReadError::Invariant(
                "content chunk page made no progress",
            )));
        }
        for chunk in page.records() {
            assembler.push(chunk).map_err(DraftSeedReadError::Content)?;
            after = Some(chunk.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    assembler.finish().map_err(DraftSeedReadError::Content)
}
