#![allow(dead_code)]

use super::host_support;
use beryl_app::composer_host::*;
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{ContentRevision, SyndicItemId, SyndicThreadId, SyndicTurnId};
use beryl_state::{AssetState, BerylState};
use std::num::NonZeroU64;
use syndic_storage::test_faults::{
    FixtureBatch, FixtureRecord, fixture_inline_paragraph_projection,
};
use syndic_storage::*;
#[path = "assets.rs"]
mod assets;
pub use assets::{publish_asset, seal_marker_set};

pub fn operation(value: u8) -> DraftPieceOperationIdV1 {
    DraftPieceOperationIdV1::from_bytes([value; 16])
}
pub fn timestamp() -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(10)
}

pub fn fixture(
    name: &str,
) -> (
    host_support::TestHome,
    HomeStore,
    BerylState,
    SyndicStorage,
    ComposerHostHistoricalTarget,
) {
    let (home, store, state, storage, thread) = host_support::fixture_with_state(name, 201);
    let payload =
        ComposerPayload::new(vec![ComposerAtom::Text("historical input".into())]).unwrap();
    let target = seed_source(&store, &storage, thread, &payload);
    (home, store, state, storage, target)
}

pub fn seed_source(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    payload: &ComposerPayload,
) -> ComposerHostHistoricalTarget {
    seed_source_with_assets(store, storage, thread, payload, None)
}

pub fn seed_source_with_assets(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    payload: &ComposerPayload,
    proof: Option<beryl_model::SealedAssetReferenceSetProof>,
) -> ComposerHostHistoricalTarget {
    seed_source_with_identities(
        store,
        storage,
        thread,
        payload,
        proof,
        (
            SyndicTurnId::from_bytes([203; 16]),
            SyndicItemId::from_bytes([204; 16]),
        ),
    )
}

pub fn seed_source_with_identities(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    payload: &ComposerPayload,
    proof: Option<beryl_model::SealedAssetReferenceSetProof>,
    (turn, item): (SyndicTurnId, SyndicItemId),
) -> ComposerHostHistoricalTarget {
    let current = host_support::current(storage.clone(), store, thread);
    let thread_revision = current.thread().revision().checked_next().unwrap();
    let digest = root_turn_chain_digest(turn);
    let path = SelectedPathProof::new(Some(turn), thread_revision, digest);
    let projection = fixture_inline_paragraph_projection(item, turn, "historical");
    let revision = projection.revision();
    let prepared = PreparedContent::composer(payload).unwrap();
    let content_revision = ContentRevision::new(1).unwrap();
    let content = prepared.reference(content_revision);
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::ContentManifest(
            prepared.sealed_manifest(content_revision),
        ))
        .unwrap();
    let mut offset = 0;
    for chunk in prepared.chunks() {
        batch
            .put(FixtureRecord::ContentChunk(chunk.clone()))
            .unwrap();
        let span = ContentByteSpanRecord::for_chunk(chunk, offset).unwrap();
        offset = span.end();
        batch.put(FixtureRecord::ContentByteSpan(span)).unwrap();
    }
    for span in prepared.text_spans() {
        batch.put(FixtureRecord::ContentTextSpan(*span)).unwrap();
    }
    for piece in prepared.pieces() {
        batch.put(FixtureRecord::ContentPiece(*piece)).unwrap();
    }
    let history = storage
        .history_summary(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    let draft = current.draft();
    let records = [
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            path,
            draft.id(),
            ThreadLineageProof::new(
                None,
                None,
                ThreadLineageDepth::FIRST,
                root_thread_lineage_digest(thread),
            ),
            None,
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft.id(),
            draft.revision(),
            thread_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            history.revision().checked_next().unwrap(),
            thread_revision,
            Some(turn),
            digest,
            false,
            SyndicTimestamp::from_unix_millis(2),
        )),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            SyndicTimestamp::from_unix_millis(2),
        )),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            proof,
        )),
        FixtureRecord::ImageLabelAuthorityHead(
            ImageLabelAuthorityHeadV1::new(
                thread,
                2,
                ImageLabelFrontier::EMPTY,
                ImageLabelFrontier::from_raw(
                    proof
                        .and_then(|p| p.sequential().maximum_image_label())
                        .map_or(0, |label| label.get()),
                ),
            )
            .unwrap(),
        ),
        FixtureRecord::DraftImageLabelProtectionHead(
            DraftImageLabelProtectionHeadV1::new(
                thread,
                2,
                ImageLabelFrontier::from_raw(
                    proof
                        .and_then(|p| p.sequential().maximum_image_label())
                        .map_or(0, |label| label.get()),
                ),
            )
            .unwrap(),
        ),
        FixtureRecord::TranscriptViewEntry(TranscriptViewEntryRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TranscriptPosition::FIRST,
            item,
            revision,
            ItemProjectionGeneration::FIRST,
            projection.id(),
            revision,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            revision,
            1,
            Some(turn),
            digest,
            ProjectionLifecycle::Current,
        )),
    ];
    for record in records {
        batch.put(record).unwrap();
    }
    host_support::committed(host_support::execute(
        store,
        storage.fixture_contribution(storage.revision(store).unwrap(), batch),
    ));
    ComposerHostHistoricalTarget {
        thread_id: thread,
        turn_id: turn,
        item_id: item,
        transcript: CurrentTranscriptEntryProof::new(
            TranscriptGeneration::FIRST,
            TranscriptPosition::FIRST,
        ),
    }
}

pub fn activate(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> SyndicComposerHost {
    activate_with_ids(store, storage, thread, 205, 206)
}

pub fn activate_with_ids(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    session: u8,
    open: u8,
) -> SyndicComposerHost {
    let mut host = SyndicComposerHost::new(storage);
    assert!(matches!(
        host.test_activate(
            store,
            ComposerHostActivationRequest::new(
                thread,
                DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
                operation(open),
                NonZeroU64::MIN,
                None,
                Box::new([])
            ),
            &CommandCancellation::new()
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    host
}

pub fn finish(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    ticket: ComposerHostHistoricalTicket,
) -> ComposerHostHistoricalCompletion {
    for _ in 0..1_000 {
        match host
            .advance_historical_replacement(
                store,
                ticket,
                assets,
                timestamp(),
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostHistoricalAdvance::Progress => {}
            ComposerHostHistoricalAdvance::Ready(completion) => return completion,
            other => panic!("unexpected replacement outcome: {other:?}"),
        }
    }
    panic!("bounded fixture failed to finish")
}
