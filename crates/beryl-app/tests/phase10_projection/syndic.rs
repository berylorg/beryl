#![allow(dead_code)]

#[path = "syndic/assets.rs"]
mod assets;
#[path = "../phase166_syndic_composer_history/support.rs"]
mod composer_support;
#[path = "syndic/exact.rs"]
mod exact;
#[path = "syndic/history.rs"]
mod history;
#[path = "../phase172_syndic_composer_publication/support.rs"]
mod publication_support;
#[path = "syndic/support.rs"]
mod support;

use beryl_app::{
    cas_projection::{
        MinimumTurnCaptureReserve, ProjectionCancellationToken, ProjectionConnectionService,
        ProjectionServiceConfig, ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
        ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionProvider,
        ScheduledOrdinaryExecutionUnavailable,
    },
    composer_host::{ComposerHostSubmissionAdvance, ComposerHostSubmissionRequest},
};
use beryl_home_store::{
    CommandCancellation, CursorReadLimits, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    CasItemId, SealedAssetReferenceSetProof, SyndicAcceptedInputId, SyndicDraftId,
    SyndicDraftMarkerId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::{AssetState, BerylState};
use support::{execute, project_item};
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
    pub(crate) user_item: SyndicItemId,
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

pub struct FixtureHome<'a>(beryl_app::cas_projection::LiveHomeCommand<'a>);

impl std::ops::Deref for FixtureHome<'_> {
    type Target = HomeStore;

    fn deref(&self) -> &Self::Target {
        self.0.home()
    }
}

impl Fixture {
    pub fn home(&self) -> FixtureHome<'_> {
        FixtureHome(self.store.live_home_command().unwrap())
    }
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
                    history_policy(),
                ),
            ),
        );
        let config = ProjectionServiceConfig::try_new(
            128,
            worker_capacity,
            MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap();
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
        let (kind, source_draft) = self.submit_text_via_composer(thread, text);
        let FirstAcceptanceKind::Idle { user_item_id } = kind else {
            panic!("fixture text submission expected an idle thread")
        };
        SubmittedTurn {
            turn: source_draft.submitted_turn_id(),
            user_item: user_item_id,
        }
    }

    pub fn accept_text(&mut self, text: &str) -> SyndicAcceptedInputId {
        let (kind, source_draft) = self.submit_text_via_composer(self.thread, text);
        let FirstAcceptanceKind::Accepted = kind else {
            panic!("fixture accepted input expected a busy thread")
        };
        source_draft.accepted_input_id()
    }

    fn submit_text_via_composer(
        &mut self,
        thread: SyndicThreadId,
        text: &str,
    ) -> (FirstAcceptanceKind, SyndicDraftId) {
        self.submit_via_composer(thread, |host, home, binding| {
            composer_support::commit_text(
                host,
                home,
                binding,
                1,
                0,
                0,
                text,
                u64::try_from(text.len()).unwrap(),
                u64::try_from(text.lines().count()).unwrap(),
            )
        })
    }

    pub(crate) fn submit_via_composer(
        &mut self,
        thread: SyndicThreadId,
        edit: impl FnOnce(
            &mut beryl_app::composer_host::SyndicComposerHost,
            &HomeStore,
            beryl_app::composer_host::ComposerHostBinding,
        ) -> beryl_app::composer_host::ComposerHostBinding,
    ) -> (FirstAcceptanceKind, SyndicDraftId) {
        let seed = self.next_draft;
        let next_draft = SyndicDraftId::from_bytes([self.next_draft; 16]);
        self.next_draft = self.next_draft.checked_add(1).unwrap();
        let user_item = SyndicItemId::from_bytes([self.next_item; 16]);
        self.next_item = self.next_item.checked_add(1).unwrap();
        let admitted_at = self.tick();
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        let assets = self.state.assets();
        let (mut host, binding) =
            composer_support::activated(self.storage, home, thread, seed, seed.wrapping_add(1));
        let edited = edit(&mut host, home, binding);
        let source_draft = edited.candidate().draft_id();
        let seals = publication_support::service(home, self.storage, assets, 1, 1);
        let ticket = host
            .begin_submission(ComposerHostSubmissionRequest::new(
                next_draft,
                user_item,
                DraftComposerMaterializationOperationIdV1::from_bytes([seed.wrapping_add(2); 16]),
                DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(3); 16]),
                admitted_at,
                Self::submission_admission_requirement(),
            ))
            .unwrap();
        for _ in 0..16_384 {
            match host
                .advance_submission(
                    home,
                    ticket,
                    assets,
                    &seals,
                    composer_support::operation_id(u64::from(seed) + 1_000),
                    None,
                    admitted_at,
                    &CommandCancellation::new(),
                )
                .unwrap()
            {
                ComposerHostSubmissionAdvance::Progress(_)
                | ComposerHostSubmissionAdvance::ReconciliationPending => {}
                ComposerHostSubmissionAdvance::ExactSuccess(kind) => return (kind, source_draft),
                outcome => panic!("fixture submission did not commit exactly: {outcome:?}"),
            }
        }
        panic!("fixture submission did not converge")
    }

    fn submission_admission_requirement() -> beryl_home_store::TurnStartAdmissionRequirement {
        beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap()
        .turn_start_admission_requirement()
    }

    pub fn submitted_content(&self, submitted: SubmittedTurn) -> ContentReference {
        self.storage
            .canonical_item(&*self.home(), submitted.user_item, point_limit())
            .unwrap()
            .unwrap()
            .presentation_content()
            .expect("submitted user fixture must retain its sealed composer content")
    }

    pub fn selected_path(&self, thread: SyndicThreadId) -> SelectedPathProof {
        let command_home = self.store.live_home_command().unwrap();
        let thread = self
            .storage
            .thread(command_home.home(), thread, point_limit())
            .unwrap()
            .unwrap();
        SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        )
    }

    pub fn retire_current_binding(&mut self, thread: SyndicThreadId) {
        let current = {
            let command_home = self.store.live_home_command().unwrap();
            self.storage
                .current_binding(command_home.home(), thread, point_limit())
                .unwrap()
                .unwrap()
        };
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
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        execute(
            home,
            self.storage.publish_stale_binding(
                self.storage.revision(home).unwrap(),
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
        let selected_path = self.selected_path(thread);
        let command_home = self.store.live_home_command().unwrap();
        let plan = self
            .storage
            .prepare_native_projection(
                command_home.home(),
                &NativeProjectionRequest::new(
                    thread,
                    selected_path,
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
        let tail = {
            let command_home = self.store.live_home_command().unwrap();
            self.storage
                .thread_tail(command_home.home(), self.thread, point_limit())
                .unwrap()
                .unwrap()
        };
        let child = SyndicThreadId::from_bytes([220; 16]);
        let created_at = tail.last_activity_at();
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        execute(
            home,
            self.storage.create_thread(
                self.storage.revision(home).unwrap(),
                CreateThread::from_tail(
                    child,
                    SyndicDraftId::from_bytes([221; 16]),
                    created_at,
                    history_policy(),
                    tail,
                )
                .unwrap(),
            ),
        );
        drop(command_home);
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
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        execute(
            home,
            self.storage.create_thread(
                self.storage.revision(home).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(),
                    created_at,
                    history_policy(),
                ),
            ),
        );
        thread
    }

    pub fn advance_unrelated_syndic_revision(&self, seed: u8) {
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        execute(
            home,
            self.storage.create_thread(
                self.storage.revision(home).unwrap(),
                CreateThread::ordinary(
                    SyndicThreadId::from_bytes([seed; 16]),
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(),
                    SyndicTimestamp::from_unix_millis(50_000 + u64::from(seed)),
                    history_policy(),
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

fn history_policy() -> DraftEditHistoryPolicyV1 {
    DraftEditHistoryPolicyV1::new(64 * 1024 * 1024, 1).unwrap()
}
