#![allow(dead_code)]

#[path = "syndic/exact.rs"]
mod exact;
#[path = "syndic/support.rs"]
mod support;

use beryl_home_store::{CursorReadLimits, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{CasItemId, SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use support::{execute, project_item, stage_prepared_content};
use syndic_storage::*;

pub use exact::execution_binding;

#[derive(Clone, Copy)]
pub struct SubmittedTurn {
    pub turn: SyndicTurnId,
}

pub struct Fixture {
    _directory: tempfile::TempDir,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: SyndicThreadId,
    next_draft: u8,
    next_item: u8,
    clock: u64,
}

impl Fixture {
    pub fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        Self::from_store(seed, directory, store)
    }

    #[cfg(feature = "test-faults")]
    pub fn with_faults(seed: u8, faults: beryl_home_store::test_faults::FaultController) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults,
        )
        .unwrap();
        Self::from_store(seed, directory, store)
    }

    fn from_store(seed: u8, directory: tempfile::TempDir, mut store: HomeStore) -> Self {
        let storage = SyndicStorage::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        execute(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    SyndicTimestamp::from_unix_millis(1),
                ),
            ),
        );
        Self {
            _directory: directory,
            store,
            storage,
            thread,
            next_draft: 40,
            next_item: 80,
            clock: 2,
        }
    }

    pub fn submit_text(&mut self, text: &str) -> SubmittedTurn {
        self.submit_text_on(self.thread, text)
    }

    pub fn submit_text_on(&mut self, thread: SyndicThreadId, text: &str) -> SubmittedTurn {
        let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
        let content = PreparedContent::composer(&payload).unwrap();
        stage_prepared_content(&self.store, self.storage, &content);
        let current = self
            .storage
            .current_draft(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare(&current, &content, self.tick()).unwrap()
        else {
            panic!("fixture submission must change the draft")
        };
        execute(
            &self.store,
            self.storage
                .update_draft_payload(self.storage.revision(&self.store).unwrap(), update),
        );
        let current = self
            .storage
            .current_draft(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = self
            .storage
            .input_gate(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let next_draft = SyndicDraftId::from_bytes([self.next_draft; 16]);
        self.next_draft = self.next_draft.checked_add(1).unwrap();
        let user_item = SyndicItemId::from_bytes([self.next_item; 16]);
        self.next_item = self.next_item.checked_add(1).unwrap();
        let submission = IdleSubmission::new(
            thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.record().revision(),
            next_draft,
            user_item,
            AdmissionMarkers::default(),
            self.tick(),
        );
        let turn = submission.submitted_turn_id();
        execute(
            &self.store,
            self.storage
                .submit_idle_draft(self.storage.revision(&self.store).unwrap(), submission),
        );
        SubmittedTurn { turn }
    }

    pub fn selected_path(&self, thread: SyndicThreadId) -> SelectedPathProof {
        let thread = self
            .storage
            .thread(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        SelectedPathProof::new(
            thread.record().committed_tail(),
            thread.record().revision(),
            thread.record().selected_path_digest(),
        )
    }

    pub fn complete_with_assistant(&mut self, submitted: SubmittedTurn, text: &str) {
        self.complete_with_assistant_on(self.thread, submitted, text);
    }

    pub fn complete_with_assistant_on(
        &mut self,
        thread: SyndicThreadId,
        submitted: SubmittedTurn,
        text: &str,
    ) {
        let started_at = self.tick();
        let source = exact::establish_turn(
            &self.store,
            self.storage,
            thread,
            submitted.turn,
            started_at,
        );
        self.admit(
            thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnActivated,
        );
        let item = SyndicItemId::from_bytes([self.next_item; 16]);
        self.next_item = self.next_item.checked_add(1).unwrap();
        let cas_item = CasItemId::new(format!("phase10-item-{item}")).unwrap();
        let descriptor = SourceItemDescriptor::new(
            item,
            cas_item.clone(),
            ProviderItemKind::AgentMessage,
            ProviderItemDisposition::CanonicalText,
        )
        .unwrap();
        self.admit(
            thread,
            submitted.turn,
            &source,
            SourceEventPayload::ItemStarted {
                item: descriptor.clone(),
                assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
            },
        );
        self.admit(
            thread,
            submitted.turn,
            &source,
            SourceEventPayload::ItemDelta {
                item_id: item,
                cas_item_id: cas_item,
                expected_kind: ProviderItemKind::AgentMessage,
                text: SourceEventText::new(text).unwrap(),
            },
        );
        self.admit(
            thread,
            submitted.turn,
            &source,
            SourceEventPayload::ItemCompleted {
                item: descriptor,
                assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
            },
        );
        self.admit(
            thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        );
        self.finalize_all(thread, submitted.turn);
    }

    pub fn retire_current_binding(&mut self, thread: SyndicThreadId) {
        let current = self
            .storage
            .current_binding(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let source = self.native_source(thread);
        let usable = source.binding();
        let stale = StaleCasBinding::new(
            usable.execution().clone(),
            usable.cas_thread_id().clone(),
            Some(usable.tool_profile()),
            Some(usable.represented_prefix()),
            Some(usable.lineage()),
            Some(usable.native_turn_count()),
            None,
            "fixture retires native lineage",
            self.tick(),
        )
        .unwrap();
        execute(
            &self.store,
            self.storage.publish_stale_binding(
                self.storage.revision(&self.store).unwrap(),
                PublishStaleBinding::new(
                    thread,
                    current.binding().revision(),
                    current.binding().selected_path(),
                    stale,
                ),
            ),
        );
    }

    pub fn native_source(&self, thread: SyndicThreadId) -> NativeProjectionSource {
        let plan = self
            .storage
            .prepare_native_projection(
                &self.store,
                &NativeProjectionRequest::new(
                    thread,
                    self.selected_path(thread),
                    execution_binding(),
                    exact::tool_profile(),
                ),
                point_limit(),
            )
            .unwrap();
        match plan {
            NativeProjectionPlan::Current { source, .. }
            | NativeProjectionPlan::Resume { source, .. }
            | NativeProjectionPlan::Fork { source, .. } => source,
            NativeProjectionPlan::Fresh { .. } | NativeProjectionPlan::Unavailable { .. } => {
                panic!("fixture expected an exact native source")
            }
        }
    }

    pub fn create_child_pending(&mut self, text: &str) -> SyndicThreadId {
        self.finish_transcript(self.thread);
        let tail = self
            .storage
            .thread_tail(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let child = SyndicThreadId::from_bytes([220; 16]);
        let created_at = tail.last_activity_at();
        execute(
            &self.store,
            self.storage.create_thread(
                self.storage.revision(&self.store).unwrap(),
                CreateThread::from_tail(
                    child,
                    SyndicDraftId::from_bytes([221; 16]),
                    created_at,
                    tail,
                )
                .unwrap(),
            ),
        );
        self.submit_text_on(child, text);
        child
    }

    pub fn create_ordinary_pending(&mut self, seed: u8, text: &str) -> SyndicThreadId {
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        let created_at = self.tick();
        execute(
            &self.store,
            self.storage.create_thread(
                self.storage.revision(&self.store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    created_at,
                ),
            ),
        );
        self.submit_text_on(thread, text);
        thread
    }

    pub fn advance_clock_to(&mut self, next_unix_millis: u64) {
        self.clock = self.clock.max(next_unix_millis);
    }

    fn admit(
        &mut self,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        source: &CasTurnSource,
        payload: SourceEventPayload,
    ) {
        let observed_at = self.tick();
        exact::admit_event(
            &self.store,
            self.storage,
            thread,
            turn,
            source,
            payload,
            observed_at,
        );
    }

    fn finalize_all(&mut self, thread: SyndicThreadId, turn: SyndicTurnId) {
        let indexes = self
            .storage
            .turn_items(
                &self.store,
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
                .canonical_item(&self.store, index.item_id(), point_limit())
                .unwrap()
                .unwrap();
            let manifest = self
                .storage
                .content_manifest(
                    &self.store,
                    item.record()
                        .payload()
                        .content()
                        .expect("finalization fixture item must own content")
                        .id(),
                    point_limit(),
                )
                .unwrap()
                .unwrap();
            if manifest.record().lifecycle() == ContentLifecycle::Live {
                let state = self
                    .storage
                    .turn_state(&self.store, turn, point_limit())
                    .unwrap()
                    .unwrap();
                let updated_at = self.tick();
                execute(
                    &self.store,
                    self.storage.freeze_next_turn_item(
                        self.storage.revision(&self.store).unwrap(),
                        FreezeNextTurnItem::new(
                            thread,
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
                .canonical_item(&self.store, index.item_id(), point_limit())
                .unwrap()
                .unwrap();
            if matches!(
                item.record().kind(),
                CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
            ) {
                project_item(&self.store, self.storage, index.item_id());
            }
            let state = self
                .storage
                .turn_state(&self.store, turn, point_limit())
                .unwrap()
                .unwrap();
            let updated_at = self.tick();
            execute(
                &self.store,
                self.storage.finalize_next_turn_item(
                    self.storage.revision(&self.store).unwrap(),
                    FinalizeNextTurnItem::new(
                        thread,
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

    fn finish_transcript(&self, thread: SyndicThreadId) {
        let thread_record = self
            .storage
            .thread(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let head = self
            .storage
            .transcript_view_head(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let generation = head.record().generation();
        execute(
            &self.store,
            self.storage.start_transcript_build(
                self.storage.revision(&self.store).unwrap(),
                StartTranscriptBuild::new(
                    thread,
                    thread_record.record().revision(),
                    head.record().revision(),
                ),
            ),
        );
        for _ in 0..1_024 {
            let build = self
                .storage
                .transcript_build(&self.store, thread, generation, point_limit())
                .unwrap()
                .unwrap();
            if build.record().phase() == TranscriptBuildPhase::Complete {
                return;
            }
            execute(
                &self.store,
                self.storage.advance_transcript_build(
                    self.storage.revision(&self.store).unwrap(),
                    AdvanceTranscriptBuild::new(thread, generation, build.record().revision()),
                ),
            );
        }
        panic!("fixture transcript build did not converge")
    }

    fn tick(&mut self) -> SyndicTimestamp {
        let value = self.clock;
        self.clock = self.clock.checked_add(1).unwrap();
        SyndicTimestamp::from_unix_millis(value)
    }
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}
