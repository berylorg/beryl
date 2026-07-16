#![allow(dead_code)]

#[path = "../phase6_live_history/support.rs"]
mod base;
#[path = "../support/exact_cas.rs"]
pub mod exact_cas;

use std::num::NonZeroU64;

use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    AssetId, CasItemId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::*;

pub use base::TestHome;

pub fn open(path: &std::path::Path) -> HomeStore {
    base::open(path)
}

fn turn_end_status(outcome: TurnTerminalOutcome) -> TurnEndStatus {
    match outcome {
        TurnTerminalOutcome::Incomplete => {
            TurnEndStatus::incomplete(TurnIncompleteReason::ItemAuditFailed)
        }
        _ => TurnEndStatus::new(outcome, None).unwrap(),
    }
}

pub fn stage_prepared_content(
    store: &HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    base::stage_prepared_content(store, storage, content);
}

#[derive(Clone, Copy)]
pub struct SubmittedTurn {
    pub turn: SyndicTurnId,
    pub user_item: SyndicItemId,
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
        execute(
            store,
            storage.create_thread(
                storage.revision(store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([2; 16]),
                    SyndicTimestamp::from_unix_millis(1),
                ),
            ),
        );
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
        let payload = if text.is_empty() {
            ComposerPayload::default()
        } else {
            ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap()
        };
        self.submit_payload(payload, AdmissionMarkers::default())
    }

    pub fn update_draft_text(&mut self, text: &str) {
        let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
        let content = PreparedContent::composer(&payload).unwrap();
        if self
            .storage
            .content_manifest(self.store, content.id(), point_limit())
            .unwrap()
            .is_none()
        {
            base::stage_prepared_content(self.store, self.storage, &content);
        }
        let current = self
            .storage
            .current_draft(self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare(&current, &content, self.tick()).unwrap()
        else {
            return;
        };
        execute(
            self.store,
            self.storage
                .update_draft_payload(self.storage.revision(self.store).unwrap(), update),
        );
    }

    pub fn submit_marker(&mut self) -> SubmittedTurn {
        let marker_id = SyndicDraftMarkerId::from_bytes([66; 16]);
        let label = ImageLabelOrdinal::FIRST;
        let payload = ComposerPayload::new(vec![
            ComposerAtom::text("image").unwrap(),
            ComposerAtom::image_marker(marker_id, label),
        ])
        .unwrap();
        let asset = AssetId::sha256_v1([67; 32], NonZeroU64::new(64).unwrap());
        let markers =
            AdmissionMarkers::new(vec![ResolvedImageMarker::new(marker_id, label, asset)]).unwrap();
        self.submit_payload(payload, markers)
    }

    fn submit_payload(
        &mut self,
        payload: ComposerPayload,
        markers: AdmissionMarkers,
    ) -> SubmittedTurn {
        let content = PreparedContent::composer(&payload).unwrap();
        if self
            .storage
            .content_manifest(self.store, content.id(), point_limit())
            .unwrap()
            .is_none()
        {
            base::stage_prepared_content(self.store, self.storage, &content);
        }
        let current = self
            .storage
            .current_draft(self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        match DraftPayloadUpdate::prepare(&current, &content, self.tick()).unwrap() {
            DraftPayloadUpdateDecision::Update(update) => execute(
                self.store,
                self.storage
                    .update_draft_payload(self.storage.revision(self.store).unwrap(), update),
            ),
            DraftPayloadUpdateDecision::NoChange => {}
        }
        let current = self
            .storage
            .current_draft(self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = self
            .storage
            .input_gate(self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let next_draft = SyndicDraftId::from_bytes([self.next_draft; 16]);
        self.next_draft = self.next_draft.checked_add(1).unwrap();
        let user_item = SyndicItemId::from_bytes([self.next_item; 16]);
        self.next_item = self.next_item.checked_add(1).unwrap();
        let submission = IdleSubmission::new(
            self.thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.record().revision(),
            next_draft,
            user_item,
            markers,
            self.tick(),
        );
        let turn = submission.submitted_turn_id();
        execute(
            self.store,
            self.storage
                .submit_idle_draft(self.storage.revision(self.store).unwrap(), submission),
        );
        SubmittedTurn { turn, user_item }
    }

    pub fn selected_path(&self) -> SelectedPathProof {
        let thread = self
            .storage
            .thread(self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        SelectedPathProof::new(
            thread.record().committed_tail(),
            thread.record().revision(),
            thread.record().selected_path_digest(),
        )
    }

    pub fn complete_without_assistant(
        &mut self,
        submitted: SubmittedTurn,
        outcome: TurnTerminalOutcome,
    ) {
        let source = self.establish_exact_cas(submitted.turn);
        self.admit_exact(submitted.turn, &source, SourceEventPayload::TurnActivated);
        self.correlate_user_input(submitted, &source);
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::TurnEnded(turn_end_status(outcome)),
        );
        self.finalize_all(submitted.turn);
    }

    pub fn complete_with_assistant(
        &mut self,
        submitted: SubmittedTurn,
        phase: AssistantMessagePhase,
        text: &str,
    ) -> SyndicItemId {
        let source = self.establish_exact_cas(submitted.turn);
        self.admit_exact(submitted.turn, &source, SourceEventPayload::TurnActivated);
        self.correlate_user_input(submitted, &source);
        let item = self.allocate_item();
        let cas_item = CasItemId::new(format!("test-item-{item}")).unwrap();
        let descriptor = SourceItemDescriptor::new(
            item,
            cas_item.clone(),
            ProviderItemKind::AgentMessage,
            ProviderItemDisposition::CanonicalText,
        )
        .unwrap();
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::ItemStarted {
                item: descriptor.clone(),
                assistant_phase: Some(phase),
            },
        );
        if !text.is_empty() {
            self.admit_exact(
                submitted.turn,
                &source,
                SourceEventPayload::ItemDelta {
                    item_id: item,
                    cas_item_id: cas_item.clone(),
                    expected_kind: ProviderItemKind::AgentMessage,
                    text: SourceEventText::new(text).unwrap(),
                },
            );
        }
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::ItemCompleted {
                item: descriptor,
                assistant_phase: Some(phase),
            },
        );
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        );
        self.finalize_all(submitted.turn);
        item
    }

    pub fn complete_with_operational(&mut self, submitted: SubmittedTurn, text: &str) {
        let source = self.establish_exact_cas(submitted.turn);
        self.admit_exact(submitted.turn, &source, SourceEventPayload::TurnActivated);
        self.correlate_user_input(submitted, &source);
        let item = self.allocate_item();
        let cas_item = CasItemId::new(format!("test-item-{item}")).unwrap();
        let descriptor = SourceItemDescriptor::new(
            item,
            cas_item.clone(),
            ProviderItemKind::CommandExecution,
            ProviderItemDisposition::CanonicalText,
        )
        .unwrap();
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::ItemStarted {
                item: descriptor.clone(),
                assistant_phase: None,
            },
        );
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::ItemDelta {
                item_id: item,
                cas_item_id: cas_item,
                expected_kind: ProviderItemKind::CommandExecution,
                text: SourceEventText::new(text).unwrap(),
            },
        );
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::ItemCompleted {
                item: descriptor,
                assistant_phase: None,
            },
        );
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        );
        self.finalize_all(submitted.turn);
    }

    fn establish_exact_cas(&mut self, turn: SyndicTurnId) -> CasTurnSource {
        let started_at = self.tick();
        exact_cas::establish_turn(self.store, self.storage, self.thread, turn, started_at)
    }

    fn admit_exact(
        &mut self,
        turn: SyndicTurnId,
        source: &CasTurnSource,
        payload: SourceEventPayload,
    ) {
        let observed_at = self.tick();
        exact_cas::admit_event(
            self.store,
            self.storage,
            self.thread,
            turn,
            source,
            payload,
            observed_at,
        );
    }

    fn correlate_user_input(&mut self, submitted: SubmittedTurn, source: &CasTurnSource) {
        let item = self
            .storage
            .canonical_item(self.store, submitted.user_item, point_limit())
            .unwrap()
            .unwrap();
        let descriptor = SourceItemDescriptor::new(
            submitted.user_item,
            CasItemId::new(format!("test-user-item-{}", submitted.user_item)).unwrap(),
            ProviderItemKind::UserMessage,
            item.record().disposition(),
        )
        .unwrap();
        self.admit_exact(
            submitted.turn,
            source,
            SourceEventPayload::ItemStarted {
                item: descriptor.clone(),
                assistant_phase: None,
            },
        );
        self.admit_exact(
            submitted.turn,
            source,
            SourceEventPayload::ItemCompleted {
                item: descriptor,
                assistant_phase: None,
            },
        );
    }

    fn allocate_item(&mut self) -> SyndicItemId {
        let item = SyndicItemId::from_bytes([self.next_item; 16]);
        self.next_item = self.next_item.checked_add(1).unwrap();
        item
    }

    fn finalize_all(&mut self, turn: SyndicTurnId) {
        let indexes = self
            .storage
            .turn_items(
                self.store,
                turn,
                None,
                CursorReadLimits::new(64, 1_000_000).unwrap(),
            )
            .unwrap()
            .records()
            .to_vec();
        for index in indexes {
            let item = self
                .storage
                .canonical_item(self.store, index.item_id(), point_limit())
                .unwrap()
                .unwrap();
            let manifest = self
                .storage
                .content_manifest(
                    self.store,
                    item.record()
                        .payload()
                        .content()
                        .expect("recovery text item has canonical content")
                        .id(),
                    point_limit(),
                )
                .unwrap()
                .unwrap();
            if manifest.record().lifecycle() == ContentLifecycle::Live {
                let state = self
                    .storage
                    .turn_state(self.store, turn, point_limit())
                    .unwrap()
                    .unwrap();
                let updated_at = self.tick();
                execute(
                    self.store,
                    self.storage.freeze_next_turn_item(
                        self.storage.revision(self.store).unwrap(),
                        FreezeNextTurnItem::new(
                            self.thread,
                            turn,
                            state.record().revision(),
                            index.ordinal(),
                            index.item_id(),
                            updated_at,
                        ),
                    ),
                );
            }
            let item = self
                .storage
                .canonical_item(self.store, index.item_id(), point_limit())
                .unwrap()
                .unwrap();
            if matches!(
                item.record().kind(),
                CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
            ) {
                project_item(self.store, self.storage, index.item_id());
            }
            let state = self
                .storage
                .turn_state(self.store, turn, point_limit())
                .unwrap()
                .unwrap();
            let updated_at = self.tick();
            execute(
                self.store,
                self.storage.finalize_next_turn_item(
                    self.storage.revision(self.store).unwrap(),
                    FinalizeNextTurnItem::new(
                        self.thread,
                        turn,
                        state.record().revision(),
                        index.ordinal(),
                        index.item_id(),
                        updated_at,
                    ),
                ),
            );
        }
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

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn project_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.record().revision(), generation),
        ),
    );
    for _ in 0..4_096 {
        if storage
            .item_projection_set(store, item, generation, point_limit())
            .unwrap()
            .is_some()
        {
            return;
        }
        let build = storage
            .item_projection_build(store, item, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.record().revision()),
            ),
        );
    }
    panic!("bounded recovery fixture item projection did not finish");
}
