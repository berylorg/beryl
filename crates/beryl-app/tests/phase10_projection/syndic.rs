#![allow(dead_code)]

#[path = "syndic/assets.rs"]
mod assets;
#[path = "syndic/exact.rs"]
mod exact;
#[path = "syndic/history.rs"]
mod history;
#[path = "syndic/support.rs"]
mod support;

use beryl_app::{
    cas_projection::{
        ProjectionCancellationToken, ProjectionConnectionService, ProjectionServiceConfig,
        ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
        ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionProvider,
        ScheduledOrdinaryExecutionUnavailable,
    },
    input_admission::{idle_submission_command, prepare_accepted_input_admission},
};
use beryl_home_store::{CursorReadLimits, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasItemId, SealedAssetReferenceSetProof, SyndicAcceptedInputId, SyndicDraftId,
    SyndicDraftMarkerId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::{AssetState, BerylState};
use support::{execute, project_item, stage_prepared_content};
use syndic_storage::*;

#[allow(unused_imports)]
pub use exact::{execution_binding, wsl_execution_binding};

pub fn correlate_captured_user_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item: SyndicItemId,
    source: &CasTurnSource,
    observed_at: SyndicTimestamp,
) {
    exact::correlate_user_item(store, storage, thread, turn, item, source, observed_at);
}

#[derive(Clone, Copy)]
pub struct SubmittedTurn {
    pub turn: SyndicTurnId,
    user_item: SyndicItemId,
}

struct UnavailableScheduledOrdinaryProvider;

impl ScheduledOrdinaryExecutionProvider for UnavailableScheduledOrdinaryProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

pub struct Fixture {
    _directory: tempfile::TempDir,
    pub store: ProjectionConnectionService,
    pub storage: SyndicStorage,
    pub state: BerylState,
    pub cancellation: ProjectionCancellationToken,
    pub thread: SyndicThreadId,
    next_draft: u8,
    next_item: u8,
    clock: u64,
}

impl Fixture {
    pub fn draft_marker_id(draft: SyndicDraftId, ordinal: u64) -> SyndicDraftMarkerId {
        assets::marker_id(draft, ordinal)
    }

    pub fn new(seed: u8) -> Self {
        Self::new_with_worker_capacity(seed, 128)
    }

    pub fn new_with_worker_capacity(seed: u8, worker_capacity: u64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        Self::from_store(seed, directory, store, worker_capacity)
    }

    pub fn new_with_scheduled_provider(
        seed: u8,
        create_provider: impl FnOnce(AssetState) -> Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        Self::from_store_with_provider(seed, directory, store, 128, create_provider)
    }

    #[cfg(feature = "test-faults")]
    pub fn new_with_scheduled_provider_and_faults(
        seed: u8,
        faults: beryl_home_store::test_faults::FaultController,
        create_provider: impl FnOnce(AssetState) -> Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Self {
        Self::new_with_scheduled_provider_faults_and_capacity(seed, faults, 128, create_provider)
    }

    #[cfg(feature = "test-faults")]
    pub fn new_with_scheduled_provider_faults_and_capacity(
        seed: u8,
        faults: beryl_home_store::test_faults::FaultController,
        worker_capacity: u64,
        create_provider: impl FnOnce(AssetState) -> Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults,
        )
        .unwrap();
        Self::from_store_with_provider(seed, directory, store, worker_capacity, create_provider)
    }

    #[cfg(feature = "test-faults")]
    pub fn with_faults(seed: u8, faults: beryl_home_store::test_faults::FaultController) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults,
        )
        .unwrap();
        Self::from_store(seed, directory, store, 128)
    }

    fn from_store(
        seed: u8,
        directory: tempfile::TempDir,
        store: HomeStore,
        worker_capacity: u64,
    ) -> Self {
        Self::from_store_with_provider(seed, directory, store, worker_capacity, |_| {
            Box::new(UnavailableScheduledOrdinaryProvider)
        })
    }

    fn from_store_with_provider(
        seed: u8,
        directory: tempfile::TempDir,
        mut store: HomeStore,
        worker_capacity: u64,
        create_provider: impl FnOnce(AssetState) -> Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Self {
        let storage = SyndicStorage::register(&mut store).unwrap();
        let state = BerylState::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        execute(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(),
                    SyndicTimestamp::from_unix_millis(1),
                ),
            ),
        );
        let config = ProjectionServiceConfig::try_new(128, worker_capacity).unwrap();
        let scheduled_provider = create_provider(state.assets());
        let store =
            ProjectionConnectionService::new(store, storage, config, scheduled_provider).unwrap();
        Self {
            _directory: directory,
            store,
            storage,
            state,
            cancellation: ProjectionCancellationToken::new(),
            thread,
            next_draft: 40,
            next_item: 80,
            clock: 2,
        }
    }

    pub fn into_service(self) -> (tempfile::TempDir, ProjectionConnectionService) {
        let Self {
            _directory,
            store,
            storage: _,
            state: _,
            cancellation: _,
            thread: _,
            next_draft: _,
            next_item: _,
            clock: _,
        } = self;
        (_directory, store)
    }

    pub fn submit_text(&mut self, text: &str) -> SubmittedTurn {
        self.submit_text_on(self.thread, text)
    }

    pub fn submit_text_on(&mut self, thread: SyndicThreadId, text: &str) -> SubmittedTurn {
        let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
        let submission = self.prepare_submission_on(thread, payload, None);
        self.execute_submission(submission)
    }

    pub fn accept_text(&mut self, text: &str) -> SyndicAcceptedInputId {
        let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
        let content = PreparedContent::composer(&payload).unwrap();
        stage_prepared_content(&self.store, self.storage, &content);
        let current = self
            .storage
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare(&current, &content, self.tick()).unwrap()
        else {
            panic!("fixture accepted input must change the draft")
        };
        execute(
            &self.store,
            self.storage
                .update_draft_payload(self.storage.revision(&self.store).unwrap(), update),
        );
        let current = self
            .storage
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = self
            .storage
            .input_gate(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let next_draft = SyndicDraftId::from_bytes([self.next_draft; 16]);
        self.next_draft = self.next_draft.checked_add(1).unwrap();
        let admission = AcceptedInputAdmission::new(
            self.thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            next_draft,
            None,
            self.tick(),
        );
        let accepted_input_id = admission.accepted_input_id();
        let prepared = prepare_accepted_input_admission(
            &self.store,
            self.storage,
            self.state.assets(),
            admission,
        )
        .unwrap();
        self.store
            .execute_accepted_input_admission(prepared)
            .unwrap();
        accepted_input_id
    }

    pub fn submit_reference_on(
        &mut self,
        thread: SyndicThreadId,
        content: ContentReference,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
    ) -> SubmittedTurn {
        let current = self
            .storage
            .current_draft(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare_reference(&current, content, self.tick()).unwrap()
        else {
            panic!("fixture submission must change the draft")
        };
        execute(
            &self.store,
            self.storage
                .update_draft_payload(self.storage.revision(&self.store).unwrap(), update),
        );
        let submission = self.prepare_current_submission_on(thread, asset_reference_set);
        self.execute_submission(submission)
    }

    fn execute_submission(&self, submission: IdleSubmission) -> SubmittedTurn {
        let turn = submission.submitted_turn_id();
        let user_item = submission.user_item_id();
        let command =
            idle_submission_command(&self.store, self.storage, self.state.assets(), submission)
                .unwrap();
        self.store.execute(command).unwrap();
        SubmittedTurn { turn, user_item }
    }

    fn prepare_submission_on(
        &mut self,
        thread: SyndicThreadId,
        payload: ComposerPayload,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
    ) -> IdleSubmission {
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
        self.prepare_current_submission_on(thread, asset_reference_set)
    }

    fn prepare_current_submission_on(
        &mut self,
        thread: SyndicThreadId,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
    ) -> IdleSubmission {
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
        IdleSubmission::new(
            thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            next_draft,
            user_item,
            asset_reference_set,
            self.tick(),
        )
    }

    pub fn selected_path(&self, thread: SyndicThreadId) -> SelectedPathProof {
        let thread = self
            .storage
            .thread(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        )
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
        let thread = self.create_ordinary(seed);
        self.submit_text_on(thread, text);
        thread
    }

    pub fn create_ordinary(&mut self, seed: u8) -> SyndicThreadId {
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        let created_at = self.tick();
        execute(
            &self.store,
            self.storage.create_thread(
                self.storage.revision(&self.store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(),
                    created_at,
                ),
            ),
        );
        thread
    }

    pub fn advance_unrelated_syndic_revision(&self, seed: u8) {
        execute(
            &self.store,
            self.storage.create_thread(
                self.storage.revision(&self.store).unwrap(),
                CreateThread::ordinary(
                    SyndicThreadId::from_bytes([seed; 16]),
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(),
                    SyndicTimestamp::from_unix_millis(50_000 + u64::from(seed)),
                ),
            ),
        );
    }

    pub fn advance_clock_to(&mut self, next_unix_millis: u64) {
        self.clock = self.clock.max(next_unix_millis);
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
