#![allow(dead_code)]

use std::num::NonZeroUsize;

#[path = "../phase6_live_history/support.rs"]
mod base;
#[path = "../support/exact_cas.rs"]
pub mod exact_cas;
#[path = "support/operational.rs"]
mod operational;
#[path = "support/terminal.rs"]
mod terminal;

use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, CasItemId, SealedAssetReferenceSetProof,
    SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use beryl_stream::PagePool;
use syndic_storage::*;

pub use base::TestHome;

pub fn recovery_page_pool(page_capacity: usize) -> PagePool {
    PagePool::new(
        NonZeroUsize::new(page_capacity).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    )
    .unwrap()
}

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

fn provider_message_phase(phase: AssistantMessagePhase) -> Option<ProviderMessagePhaseV1> {
    match phase {
        AssistantMessagePhase::Commentary => Some(ProviderMessagePhaseV1::Commentary),
        AssistantMessagePhase::FinalAnswer => Some(ProviderMessagePhaseV1::FinalAnswer),
        AssistantMessagePhase::Unknown => None,
    }
}

fn assistant_item(phase: AssistantMessagePhase, text: &str) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: provider_message_phase(phase),
        memory_citation: None,
    })
}

fn command_item(
    status: ProviderCommandStatusV1,
    aggregated_output: Option<&str>,
) -> ProviderItemV1 {
    let completed = status == ProviderCommandStatusV1::Completed;
    ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
        command: ProviderTextV1::inline("fixture command"),
        cwd: ProviderTextV1::inline("C:\\syndic-recovery-fixture"),
        process_id: None,
        source: ProviderCommandSourceV1::Agent,
        status,
        command_actions: Vec::new(),
        aggregated_output: aggregated_output.map(ProviderTextV1::inline),
        exit_code: completed.then_some(0),
        duration_ms: completed.then_some(1),
    })
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
                    exact_cas::execution_binding(),
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
        self.submit_payload(payload)
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
        self.submit_payload(payload)
    }

    fn submit_payload(&mut self, payload: ComposerPayload) -> SubmittedTurn {
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
        let source = current.draft().content().sealed_marker_summary().unwrap();
        let asset_reference_set = (source.marker_count() != 0).then(|| {
            SealedAssetReferenceSetProof::new(
                AssetReferenceSetId::from_bytes([68; 16]),
                source,
                source.marker_count(),
                AssetReferenceSetDigest::from_bytes([69; 32]),
            )
            .unwrap()
        });
        let submission = IdleSubmission::new(
            self.thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            next_draft,
            user_item,
            asset_reference_set,
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
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
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
        terminal::release(self.store, self.storage, self.thread, submitted.turn);
    }

    pub fn activate_without_terminal(&mut self, submitted: SubmittedTurn) -> CasTurnSource {
        let source = self.establish_exact_cas(submitted.turn);
        self.admit_exact(submitted.turn, &source, SourceEventPayload::TurnActivated);
        self.correlate_user_input(submitted, &source);
        source
    }

    pub fn finalize_turn(&mut self, turn: SyndicTurnId) {
        self.finalize_all(turn);
        terminal::release(self.store, self.storage, self.thread, turn);
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
        let started_at = self.tick();
        exact_cas::admit_item_frame(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::FIRST,
                cas_item.clone(),
                ProviderItemObservationV1::Started {
                    observed_at: ProviderLifecycleTimestampMsV1::new(started_at.unix_millis()),
                    item: assistant_item(phase, ""),
                },
            ),
            started_at,
        );
        let completion_ordinal = if text.is_empty() {
            ProviderFrameOrdinalV1::new(2).unwrap()
        } else {
            let observed_at = self.tick();
            exact_cas::admit_item_frame(
                self.store,
                self.storage,
                self.thread,
                submitted.turn,
                item,
                &source,
                ProviderItemFrameV1::new(
                    ProviderFrameOrdinalV1::new(2).unwrap(),
                    cas_item.clone(),
                    ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                        delta: ProviderTextV1::inline(text),
                    }),
                ),
                observed_at,
            );
            ProviderFrameOrdinalV1::new(3).unwrap()
        };
        let completed_at = self.tick();
        exact_cas::admit_item_frame(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                completion_ordinal,
                cas_item,
                ProviderItemObservationV1::Completed {
                    observed_at: ProviderLifecycleTimestampMsV1::new(completed_at.unix_millis()),
                    item: assistant_item(phase, text),
                },
            ),
            completed_at,
        );
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        );
        self.finalize_all(submitted.turn);
        terminal::release(self.store, self.storage, self.thread, submitted.turn);
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
                    item.provider_content()
                        .expect("completed recovery item has provider content")
                        .id(),
                    point_limit(),
                )
                .unwrap()
                .unwrap();
            if manifest.lifecycle() == ContentLifecycle::Live {
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
                            state.revision(),
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
            if item.projection_source().is_some() {
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
                        state.revision(),
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
            StartItemProjectionBuild::new(item, canonical.revision(), generation),
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
                AdvanceItemProjectionBuild::new(item, generation, build.revision()),
            ),
        );
    }
    panic!("bounded recovery fixture item projection did not finish");
}
