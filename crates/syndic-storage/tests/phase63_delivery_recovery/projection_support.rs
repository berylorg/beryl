use std::num::NonZeroUsize;

use beryl_home_store::HomeStore;
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, ContentRevision, ImageLabelOrdinal,
    SealedAssetReferenceSetProof, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use beryl_stream::PagePool;
use syndic_storage::{
    CasTurnSource, ComposerAtom, ComposerPayload, PreparedContent, SelectedPathProof,
    SourceEventPayload, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnEndStatus,
    TurnIncompleteReason, TurnTerminalOutcome,
};

pub use crate::support::{TestHome, open};

use crate::support::{exact_cas, seed_canonical_empty_thread};

#[derive(Clone, Copy)]
pub struct SubmittedTurn {
    pub turn: SyndicTurnId,
    user_item: SyndicItemId,
}

pub struct Builder<'a> {
    store: &'a HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    next_draft: u8,
    next_item: u8,
    clock: u64,
}

impl<'a> Builder<'a> {
    pub fn new(store: &'a HomeStore, storage: SyndicStorage, thread_byte: u8) -> Self {
        let thread = SyndicThreadId::from_bytes([thread_byte; 16]);
        seed_canonical_empty_thread(store, storage, thread, SyndicDraftId::from_bytes([2; 16]));
        Self {
            store,
            storage,
            thread,
            next_draft: 3,
            next_item: 80,
            clock: 2,
        }
    }

    pub const fn thread(&self) -> SyndicThreadId {
        self.thread
    }

    pub fn submit_text(&mut self, text: &str) -> SubmittedTurn {
        let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
        self.submit_payload(payload)
    }

    pub fn submit_marker(&mut self) -> SubmittedTurn {
        let payload = ComposerPayload::new(vec![
            ComposerAtom::text("image").unwrap(),
            ComposerAtom::image_marker(
                SyndicDraftMarkerId::from_bytes([66; 16]),
                ImageLabelOrdinal::FIRST,
            ),
        ])
        .unwrap();
        self.submit_payload(payload)
    }

    fn submit_payload(&mut self, payload: ComposerPayload) -> SubmittedTurn {
        let content = PreparedContent::composer(&payload).unwrap();
        let asset_reference_set = (content.summary().image_marker_count() != 0).then(|| {
            let summary = content
                .reference(ContentRevision::new(1).unwrap())
                .sealed_marker_summary()
                .unwrap();
            SealedAssetReferenceSetProof::new(
                AssetReferenceSetId::from_bytes([68; 16]),
                summary,
                summary.marker_count(),
                AssetReferenceSetDigest::from_bytes([69; 32]),
            )
            .unwrap()
        });
        let next_draft = SyndicDraftId::from_bytes([self.next_draft; 16]);
        self.next_draft = self.next_draft.checked_add(1).unwrap();
        let user_item = SyndicItemId::from_bytes([self.next_item; 16]);
        self.next_item = self.next_item.checked_add(1).unwrap();
        let turn = exact_cas::submit_prepared_current_draft(
            self.store,
            self.storage,
            self.thread,
            next_draft,
            user_item,
            &content,
            asset_reference_set,
            self.tick(),
        );
        SubmittedTurn { turn, user_item }
    }

    pub fn selected_path(&self) -> SelectedPathProof {
        self.storage
            .thread(self.store, self.thread, point_limit())
            .unwrap()
            .unwrap()
            .selected_path()
    }

    pub fn complete_without_assistant(
        &mut self,
        submitted: SubmittedTurn,
        outcome: TurnTerminalOutcome,
    ) {
        let source = self.activate_without_terminal(submitted);
        exact_cas::admit_event(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnEnded(turn_end_status(outcome)),
            self.tick(),
        );
        self.finalize_turn(submitted.turn);
    }

    pub fn activate_without_terminal(&mut self, submitted: SubmittedTurn) -> CasTurnSource {
        let source = exact_cas::establish_turn(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            self.tick(),
        );
        exact_cas::admit_event(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnActivated,
            self.tick(),
        );
        exact_cas::correlate_user_item(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            submitted.user_item,
            &source,
            self.tick(),
        );
        source
    }

    pub fn finalize_turn(&mut self, turn: SyndicTurnId) {
        exact_cas::converge_and_release_terminal_history(
            self.store,
            self.storage,
            self.thread,
            turn,
        );
    }

    fn tick(&mut self) -> SyndicTimestamp {
        let current = self.clock;
        self.clock = self.clock.checked_add(1).unwrap();
        SyndicTimestamp::from_unix_millis(current)
    }
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub fn recovery_page_pool(page_capacity: usize) -> PagePool {
    PagePool::new(
        NonZeroUsize::new(page_capacity).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    )
    .unwrap()
}

fn turn_end_status(outcome: TurnTerminalOutcome) -> TurnEndStatus {
    match outcome {
        TurnTerminalOutcome::Incomplete => {
            TurnEndStatus::incomplete(TurnIncompleteReason::ItemAuditFailed)
        }
        _ => TurnEndStatus::new(outcome, None).unwrap(),
    }
}
