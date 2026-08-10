use std::{
    convert::Infallible,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_backend::{DynamicToolCallResponse, ThreadStartOptions, TurnStartOptions};
use beryl_model::{
    CasItemId, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount, CasThreadId,
    CasTurnId, ExecutionBinding, PathFlavor, RootId, RuntimeMode, RuntimeNativePath,
    SyndicAcceptedInputId, SyndicContentId, SyndicDraftId, SyndicExecutionSnapshotId,
    SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::AssetState;
use syndic_storage::{
    AcceptedInputAdmission, AcceptedInputLifecycle, AcceptedRouteEffectiveState,
    AcceptedRouteRevision, ActivateBinding, AdvanceItemProjectionBuild, AdvanceTranscriptBuild,
    BindingState, CasItemSource, CasLineageProof, CasRepresentedPrefixProof, CasTurnSource,
    ComposerAtom, ComposerPayload, CompleteTerminalHistory, ContentAppend, ContentBuild,
    ContentLifecycle, DraftPayloadUpdate, DraftPayloadUpdateDecision, FinalizeNextTurnItem,
    FreezeNextTurnItem, IdleSubmission, InputGateState, ItemProjectionGeneration, CreateThread,
    LiveSourceEvent, NativeCasLineage, NextTurnReason, PreparedContent, ProviderFrameOrdinalV1,
    ProviderFramePreparationPlan, ProviderItemBuildLifecycle, ProviderItemFrameV1,
    ProviderItemObservationV1, ProviderItemV1, ProviderLifecycleTimestampMsV1,
    ProviderSubmittedContentV1, ProviderUserMessageV1, PublishActiveCasTurn, PublishValidBinding,
    SealedProviderFrameReference, SourceEventPayload, SourceEventSequence, StartItemProjectionBuild,
    StartTranscriptBuild, SyndicPointReadLimit, SyndicTimestamp, TranscriptBuildPhase,
    TurnEndStatus, TurnIncompleteReason, TurnTerminalOutcome, empty_selected_path_digest,
    prepare_provider_frame, stage_provider_frame,
};

use crate::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{
        OrdinaryDynamicToolAuthority, OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers,
        OrdinaryTurnExecutionRequest, ScheduledOrdinaryRequestPolicy,
        ScheduledProjectionSessionAuthority,
    },
    conversation_tools::ConversationToolRegistry,
    input_admission::{idle_submission_command, prepare_accepted_input_admission},
};

const PHASE96_EXECUTION_ROOT: &str = r"C:\work\beryl";

#[derive(Clone)]
struct Phase96SessionSlot(Arc<Mutex<Option<AdmittedProjectionSession>>>);

impl Phase96SessionSlot {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }

    fn replace(&self, session: AdmittedProjectionSession) {
        assert!(self.0.lock().unwrap().replace(session).is_none());
    }

    fn take(&self) -> Option<AdmittedProjectionSession> {
        self.0.lock().unwrap().take()
    }
}

struct Phase96ReturningSession {
    session: Option<AdmittedProjectionSession>,
    slot: Phase96SessionSlot,
}

impl ScheduledProjectionSessionAuthority for Phase96ReturningSession {
    fn session(&mut self) -> &mut AdmittedProjectionSession {
        self.session.as_mut().expect("the issued session remains owned")
    }
}

impl Drop for Phase96ReturningSession {
    fn drop(&mut self) {
        self.slot.replace(
            self.session
                .take()
                .expect("the scheduled session returns exactly once"),
        );
    }
}

struct Phase96LifecycleHandler;

impl LifecycleYieldRequestHandler for Phase96LifecycleHandler {
    fn respond_lifecycle_yield(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused Phase 96 lifecycle handler")
    }
}

struct Phase96BranchHandler;

impl BranchDiscussionResolutionRequestHandler for Phase96BranchHandler {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused Phase 96 branch handler")
    }
}

struct Phase96ToolAuthority {
    lifecycle: Phase96LifecycleHandler,
    branch: Phase96BranchHandler,
}

impl OrdinaryDynamicToolAuthority for Phase96ToolAuthority {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        OrdinaryDynamicToolHandlers::new(&mut self.lifecycle, &mut self.branch)
    }
}

struct Phase96ReadyProvider {
    slot: Phase96SessionSlot,
    assets: AssetState,
    probes: Arc<AtomicUsize>,
    issues: Arc<AtomicUsize>,
    shutdowns: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for Phase96ReadyProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let Some(session) = self.slot.take() else {
            self.probes.fetch_add(1, Ordering::SeqCst);
            return Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::SessionBusy));
        };
        self.issues.fetch_add(1, Ordering::SeqCst);
        admission
            .issue(
                Box::new(Phase96ReturningSession {
                    session: Some(session),
                    slot: self.slot.clone(),
                }),
                phase96_request_policy(),
                self.assets,
                Box::new(Phase96ToolAuthority {
                    lifecycle: Phase96LifecycleHandler,
                    branch: Phase96BranchHandler,
                }),
            )
            .map(ScheduledOrdinaryAdmissionResult::Issued)
    }

    fn shutdown(&mut self) {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        self.slot.take();
    }
}

struct Phase96ReplacementProvider {
    issues: Arc<AtomicUsize>,
    shutdowns: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for Phase96ReplacementProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.issues.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

fn phase96_request_policy() -> ScheduledOrdinaryRequestPolicy {
    ScheduledOrdinaryRequestPolicy::new(
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        Duration::from_secs(10),
        OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), Duration::from_secs(10)),
    )
}

#[derive(Clone, Copy)]
struct Phase96RecordIds {
    thread: SyndicThreadId,
    accepted_input: SyndicAcceptedInputId,
}

fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn phase96_execution_binding(runtime_id: RuntimeId) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([206; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            PHASE96_EXECUTION_ROOT,
        )
        .unwrap(),
    )
}

fn phase96_execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn phase96_stage_content(
    store: &HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    phase96_execute(
        store,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        manifest = append.next_manifest().clone();
        phase96_execute(
            store,
            storage.append_content(storage.revision(store).unwrap(), append),
        );
    }
}

#[derive(Clone, Copy)]
struct Phase96SubmittedTurn {
    turn: SyndicTurnId,
    user_item: SyndicItemId,
}

fn phase96_submit_parent(
    service: &ProjectionConnectionService,
    storage: SyndicStorage,
    state: BerylState,
    thread: SyndicThreadId,
) -> Phase96SubmittedTurn {
    let content = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text("Phase 96 canonical parent").unwrap()])
            .unwrap(),
    )
    .unwrap();
    let command_home = service.live_home_command().unwrap();
    let home = command_home.home();
    phase96_stage_content(home, storage, &content);
    let current = storage
        .current_draft(home, thread, point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &content, time(96_002)).unwrap()
    else {
        panic!("the Phase 96 parent submission must change the draft")
    };
    phase96_execute(
        home,
        storage.update_draft_payload(storage.revision(home).unwrap(), update),
    );
    let current = storage
        .current_draft(home, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(home, thread, point_limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([208; 16]),
        SyndicItemId::from_bytes([209; 16]),
        None,
        time(96_003),
    );
    let turn = submission.submitted_turn_id();
    let user_item = submission.user_item_id();
    home.execute(idle_submission_command(home, storage, state.assets(), submission).unwrap())
        .unwrap();
    Phase96SubmittedTurn { turn, user_item }
}

fn phase96_tool_profile() -> beryl_model::CasConversationToolProfile {
    ConversationToolRegistry::canonical().profile()
}

fn phase96_loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(1).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

fn phase96_establish_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    execution: ExecutionBinding,
) -> CasTurnSource {
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    assert_eq!(selected.tail(), Some(turn));
    let turn_record = storage.turn(store, turn, point_limit()).unwrap().unwrap();
    let (parent, parent_digest) = match turn_record.parent().turn() {
        Some(parent) => {
            let parent = storage.turn(store, parent, point_limit()).unwrap().unwrap();
            (Some(parent.id()), parent.chain_digest())
        }
        None => (None, empty_selected_path_digest()),
    };
    let represented =
        CasRepresentedPrefixProof::new(parent, selected.thread_revision(), parent_digest);
    let cas_thread = CasThreadId::new(format!("phase96-source-{thread}")).unwrap();
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    phase96_execute(
        store,
        storage.publish_valid_binding(
            storage.revision(store).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution,
                cas_thread.clone(),
                represented,
                CasNativeTurnCount::ZERO,
                phase96_tool_profile(),
                lineage,
            ),
        ),
    );
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let snapshot = SyndicExecutionSnapshotId::from_bytes(*turn.as_bytes());
    phase96_execute(
        store,
        storage.activate_binding(
            storage.revision(store).unwrap(),
            ActivateBinding::new(
                thread,
                binding.binding().revision(),
                gate.revision(),
                selected,
                snapshot,
                turn,
                phase96_loaded_generation(),
                time(96_004),
            ),
        ),
    );
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let cas_turn = CasTurnId::new(format!("phase96-turn-{turn}")).unwrap();
    phase96_execute(
        store,
        storage.publish_active_cas_turn(
            storage.revision(store).unwrap(),
            PublishActiveCasTurn::new(
                thread,
                binding.binding().revision(),
                gate.revision(),
                snapshot,
                cas_thread.clone(),
                cas_turn.clone(),
                time(96_004),
            ),
        ),
    );
    CasTurnSource::new(cas_thread, cas_turn)
}

fn phase96_admit_event(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let event = LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(source.clone()),
        payload,
        observed_at,
    )
    .unwrap();
    phase96_execute(
        store,
        storage.admit_live_source_event(storage.revision(store).unwrap(), event),
    );
}

fn phase96_provider_content_id(item_id: SyndicItemId) -> SyndicContentId {
    let mut bytes = *item_id.as_bytes();
    for byte in &mut bytes {
        *byte ^= 0xa5;
    }
    SyndicContentId::from_bytes(bytes)
}

fn phase96_admit_item_frame(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    frame: ProviderItemFrameV1,
    observed_at: SyndicTimestamp,
) -> SealedProviderFrameReference {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let source_event = SourceEventSequence::new(state.source_event_count() + 1).unwrap();
    let prior = storage
        .canonical_item(store, item_id, point_limit())
        .unwrap()
        .and_then(|item| item.provider().cloned());
    let item_source = CasItemSource::new(source.clone(), frame.item_id().clone());
    let plan = match prior {
        Some(prior) => ProviderFramePreparationPlan::subsequent(
            item_id,
            turn,
            item_source,
            source_event,
            prior,
            frame,
        ),
        None => ProviderFramePreparationPlan::first(
            item_id,
            turn,
            item_source,
            source_event,
            phase96_provider_content_id(item_id),
            frame,
        ),
    };
    let prepared = prepare_provider_frame(plan).unwrap();
    phase96_execute(
        store,
        storage.begin_provider_frame_build(storage.revision(store).unwrap(), &prepared),
    );
    let mut build = stage_provider_frame(
        &prepared,
        prepared.initial_build().clone(),
        &mut |batch: &syndic_storage::ProviderFrameStageBatch| {
            phase96_execute(
                store,
                storage.stage_provider_frame_batch(storage.revision(store).unwrap(), batch.clone()),
            );
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
    for _ in 0..4_096 {
        if build.lifecycle() == ProviderItemBuildLifecycle::Sealed {
            let sealed = prepared.target().clone();
            assert_eq!(build.target(), &sealed);
            phase96_admit_event(
                store,
                storage,
                thread,
                turn,
                source,
                SourceEventPayload::ItemFrame {
                    item_id,
                    frame: Box::new(sealed.clone()),
                },
                observed_at,
            );
            return sealed;
        }
        phase96_execute(
            store,
            storage.compare_provider_completion(storage.revision(store).unwrap(), build),
        );
        build = storage
            .provider_item_build(store, item_id, point_limit())
            .unwrap()
            .unwrap();
    }
    panic!("the Phase 96 provider-frame build did not finish")
}

fn phase96_correlate_user_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    submitted: Phase96SubmittedTurn,
    source: &CasTurnSource,
) {
    let item = storage
        .canonical_item(store, submitted.user_item, point_limit())
        .unwrap()
        .unwrap();
    let content = item
        .presentation_content()
        .expect("the canonical Phase 96 user item owns sealed content");
    let cas_item = CasItemId::new(format!("phase96-user-{}", submitted.user_item)).unwrap();
    let provider_item = ProviderItemV1::UserMessage(ProviderUserMessageV1 {
        client_id: None,
        submitted: ProviderSubmittedContentV1 { content },
    });
    for (ordinal, observation) in [
        (
            ProviderFrameOrdinalV1::FIRST,
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(96_006),
                item: provider_item.clone(),
            },
        ),
        (
            ProviderFrameOrdinalV1::new(2).unwrap(),
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(96_006),
                item: provider_item,
            },
        ),
    ] {
        phase96_admit_item_frame(
            store,
            storage,
            thread,
            submitted.turn,
            submitted.user_item,
            source,
            ProviderItemFrameV1::new(ordinal, cas_item.clone(), observation),
            time(96_006),
        );
    }
}

fn phase96_project_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    phase96_execute(
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
        phase96_execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.revision()),
            ),
        );
    }
    panic!("the Phase 96 item projection did not converge")
}

fn phase96_finish_transcript(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) {
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let generation = head.generation();
    phase96_execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread, thread_record.revision(), head.revision()),
        ),
    );
    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        phase96_execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread, generation, build.revision()),
            ),
        );
    }
    panic!("the Phase 96 transcript build did not converge")
}

fn phase96_finalize_parent(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    submitted: Phase96SubmittedTurn,
) {
    let indexes = storage
        .turn_items(
            store,
            submitted.turn,
            None,
            beryl_home_store::CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec();
    assert!(!indexes.is_empty(), "the canonical parent owns real items");
    for index in indexes {
        let item = storage
            .canonical_item(store, index.item_id(), point_limit())
            .unwrap()
            .unwrap();
        let content = item
            .provider_content()
            .or_else(|| item.presentation_content())
            .expect("the canonical parent item owns closed content");
        let manifest = storage
            .content_manifest(store, content.id(), point_limit())
            .unwrap()
            .unwrap();
        if manifest.lifecycle() == ContentLifecycle::Live {
            let state = storage
                .turn_state(store, submitted.turn, point_limit())
                .unwrap()
                .unwrap();
            phase96_execute(
                store,
                storage.freeze_next_turn_item(
                    storage.revision(store).unwrap(),
                    FreezeNextTurnItem::new(
                        thread,
                        submitted.turn,
                        state.revision(),
                        index.ordinal(),
                        index.item_id(),
                        time(96_010),
                    ),
                ),
            );
        }
        phase96_project_item(store, storage, index.item_id());
        let state = storage
            .turn_state(store, submitted.turn, point_limit())
            .unwrap()
            .unwrap();
        phase96_execute(
            store,
            storage.finalize_next_turn_item(
                storage.revision(store).unwrap(),
                FinalizeNextTurnItem::new(
                    thread,
                    submitted.turn,
                    state.revision(),
                    index.ordinal(),
                    index.item_id(),
                    time(96_011),
                ),
            ),
        );
    }
    phase96_finish_transcript(store, storage, thread);
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    store
        .execute_current(storage.current_complete_terminal_history(
            CompleteTerminalHistory::new(
                thread,
                submitted.turn,
                gate,
                state.revision(),
                head.generation(),
                head.revision(),
            ),
        ))
        .unwrap();
}

fn phase96_activate_parent(
    service: &ProjectionConnectionService,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    submitted: Phase96SubmittedTurn,
    execution: ExecutionBinding,
) -> CasTurnSource {
    let command_home = service.live_home_command().unwrap();
    let home = command_home.home();
    let source = phase96_establish_turn(home, storage, thread, submitted.turn, execution);
    phase96_admit_event(
        home,
        storage,
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnActivated,
        time(96_005),
    );
    phase96_correlate_user_item(home, storage, thread, submitted, &source);
    phase96_admit_event(
        home,
        storage,
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::UnknownTerminal,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        time(96_007),
    );
    source
}

fn phase96_admit_next_input(
    service: &ProjectionConnectionService,
    storage: SyndicStorage,
    state: BerylState,
    thread: SyndicThreadId,
) -> SyndicAcceptedInputId {
    let content = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text(admission_server::SUBMITTED_TEXT).unwrap()])
            .unwrap(),
    )
    .unwrap();
    let (accepted_input, prepared) = {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        phase96_stage_content(home, storage, &content);
        let current = storage
            .current_draft(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare(&current, &content, time(96_008)).unwrap()
        else {
            panic!("the Phase 96 accepted input must change the draft")
        };
        phase96_execute(
            home,
            storage.update_draft_payload(storage.revision(home).unwrap(), update),
        );
        let current = storage
            .current_draft(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let admission = AcceptedInputAdmission::new(
            thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            SyndicDraftId::from_bytes([210; 16]),
            None,
            time(96_009),
        );
        let accepted_input = admission.accepted_input_id();
        let prepared = prepare_accepted_input_admission(
            home,
            storage,
            state.assets(),
            admission,
        )
        .unwrap();
        (accepted_input, prepared)
    };
    service.execute_accepted_input_admission(prepared).unwrap();
    accepted_input
}

fn phase96_complete_parent(
    service: &ProjectionConnectionService,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    submitted: Phase96SubmittedTurn,
    source: &CasTurnSource,
) {
    let command_home = service.live_home_command().unwrap();
    let home = command_home.home();
    phase96_admit_event(
        home,
        storage,
        thread,
        submitted.turn,
        source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        time(96_010),
    );
    phase96_finalize_parent(home, storage, thread, submitted);
    home.validate_registered_domains().unwrap();
}

fn phase96_route_entry(
    store: &HomeStore,
    storage: SyndicStorage,
    ids: Phase96RecordIds,
) -> syndic_storage::AcceptedRouteEntry {
    let input = storage
        .accepted_input(store, ids.accepted_input, point_limit())
        .unwrap()
        .expect("the immutable accepted-input receipt remains present");
    for revision in 1..=8 {
        match storage.accepted_route_page(
            store,
            ids.thread,
            input.route_generation(),
            AcceptedRouteRevision::new(revision).unwrap(),
            None,
        ) {
            Ok(page) => {
                if let Some(entry) = page
                    .records()
                    .iter()
                    .find(|entry| entry.input().id() == ids.accepted_input)
                {
                    return entry.clone();
                }
            }
            Err(syndic_storage::SyndicReadError::StaleAcceptedRoute) => {}
            Err(error) => panic!("Phase 96 route read failed: {error}"),
        }
    }
    panic!("the accepted-input route remains bounded and addressable")
}

struct Phase96Fixture {
    _directory: tempfile::TempDir,
    faults: FaultController,
    state: BerylState,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    submitted: Phase96SubmittedTurn,
    source: CasTurnSource,
    probes: Arc<AtomicUsize>,
    issues: Arc<AtomicUsize>,
    shutdowns: Arc<AtomicUsize>,
    slot: Phase96SessionSlot,
    service: ProjectionConnectionService,
}

fn phase96_service() -> Phase96Fixture {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let assets = state.assets();
    let thread = SyndicThreadId::from_bytes([206; 16]);
    let execution = phase96_execution_binding(RuntimeId::from_bytes([206; 16]));
    phase96_execute(
        &home,
        storage.create_thread(
            storage.revision(&home).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([207; 16]),
                execution.clone(),
                time(96_001),
            ),
        ),
    );
    let probes = Arc::new(AtomicUsize::new(0));
    let issues = Arc::new(AtomicUsize::new(0));
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let slot = Phase96SessionSlot::new();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 6).unwrap(),
        Box::new(Phase96ReadyProvider {
            slot: slot.clone(),
            assets,
            probes: Arc::clone(&probes),
            issues: Arc::clone(&issues),
            shutdowns: Arc::clone(&shutdowns),
        }),
    )
    .unwrap();
    wait_until("the Phase 96 empty startup lanes to hand back fully", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        diagnostics.recovery_handed_off()
            && diagnostics.workers_active() == 0
            && !diagnostics.fatal()
            && probes.load(Ordering::SeqCst) == 0
            && service.worker_pool_diagnostics().active() == 0
    });
    let startup = service.accepted_input_scheduler_diagnostics();
    assert_eq!(startup.startup_pending_turns(), 0);
    assert_eq!(startup.recovered_pending_pass_count(), 0);
    assert_eq!(startup.recovered_pending_execution_unavailable(), 0);
    assert!(!startup.next_retained_source_cursor());
    assert!(!startup.next_retained_candidate_cursor());
    assert_eq!(startup.workers_started(), 0);
    assert_eq!(probes.load(Ordering::SeqCst), 0);
    assert_eq!(issues.load(Ordering::SeqCst), 0);
    assert_eq!(service.worker_pool_diagnostics().active(), 0);
    let submitted = phase96_submit_parent(&service, storage, state, thread);
    let source = phase96_activate_parent(&service, storage, thread, submitted, execution);
    Phase96Fixture {
        _directory: directory,
        faults,
        state,
        storage,
        thread,
        submitted,
        source,
        probes,
        issues,
        shutdowns,
        slot,
        service,
    }
}

#[test]
fn phase96_exact_gate_close_rejects_queued_old_command_before_closed_gate_adoption() {
    let Phase96Fixture {
        _directory,
        faults,
        state,
        storage,
        thread,
        submitted,
        source,
        probes,
        issues,
        shutdowns,
        slot,
        service,
    } = phase96_service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let runtime_id = RuntimeId::from_bytes([206; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(96_206).unwrap(),
            Path::new(PHASE96_EXECUTION_ROOT),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    assert_eq!(service.worker_pool_diagnostics().active(), 2);
    assert_eq!(service.registered_connection_count_for_test(), 1);
    let connection = Arc::clone(session.connection());
    let stable_identity = connection.identity_observation();
    let registry_owner = SyndicThreadId::from_bytes([211; 16]);
    let registry_cas_thread = CasThreadId::new("phase-96-registry-anchor").unwrap();
    let registry_lease = phase79_register_candidate_lease(
        &service,
        &connection,
        registry_cas_thread.clone(),
        registry_owner,
    );
    let registry_coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let registry_projection = LoadedCasProjection::new(
        &registry_coordinator,
        registry_owner,
        BindingRevision::new(1).unwrap(),
        phase79_execution_binding(runtime_id, 212),
        registry_cas_thread,
        registry_lease,
        phase79_lineage(),
    );
    wait_until("the Phase 96 registry anchor to charge the old service", || {
        service.worker_pool_diagnostics().active() == 3
    });
    slot.replace(session);
    let pause = connection.pause_stable_driver_before_next_cycle_for_test();
    pause.wait_until_reached();
    let accepted_input = phase96_admit_next_input(&service, storage, state, thread);
    phase96_complete_parent(&service, storage, thread, submitted, &source);
    let ids = Phase96RecordIds {
        thread,
        accepted_input,
    };
    let home = service.home.as_deref().unwrap();
    let admitted_before = phase96_route_entry(home, storage, ids);
    assert_eq!(
        admitted_before.effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::UnknownTerminal)
    );
    assert_eq!(
        admitted_before.leaf().lifecycle(),
        AcceptedInputLifecycle::Admitted
    );
    let idle_gate = storage
        .input_gate(home, ids.thread, point_limit())
        .unwrap()
        .expect("the recovered accepted-next input retains its idle gate");
    assert_eq!(idle_gate.state(), &InputGateState::Idle);
    assert_eq!(probes.load(Ordering::SeqCst), 0);
    assert_eq!(issues.load(Ordering::SeqCst), 0);
    let scheduler_at_cut = service.accepted_input_scheduler_diagnostics();
    let wake_count_at_cut = scheduler_at_cut.wake_count();
    let next_pass_at_cut = scheduler_at_cut.next_pass_count();
    let next_source_reads_at_cut = scheduler_at_cut.next_source_page_reads();
    let next_candidate_reads_at_cut = scheduler_at_cut.next_candidate_page_reads();
    let next_unavailable_at_cut = scheduler_at_cut.next_execution_unavailable();
    let next_capacity_waits_at_cut = scheduler_at_cut.next_capacity_waits();
    let next_flight_waits_at_cut = scheduler_at_cut.next_flight_waits();
    let workers_started_at_cut = scheduler_at_cut.workers_started();
    service.notify_scheduled_ordinary_execution_ready();

    let scheduler_after_wake = {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            let ring = pause.diagnostics();
            assert!(
                !diagnostics.stopped() && !diagnostics.fatal(),
                "Phase 96 next-lane wake stopped or failed: scheduler={diagnostics:?}, \
                 probes={}, issues={}, pool={:?}, ring={ring:?}, cut=(wake={}, next_pass={}, \
                 source_reads={}, candidate_reads={}, unavailable={}, capacity_waits={}, \
                 flight_waits={}, workers_started={})",
                probes.load(Ordering::SeqCst),
                issues.load(Ordering::SeqCst),
                service.worker_pool_diagnostics(),
                wake_count_at_cut,
                next_pass_at_cut,
                next_source_reads_at_cut,
                next_candidate_reads_at_cut,
                next_unavailable_at_cut,
                next_capacity_waits_at_cut,
                next_flight_waits_at_cut,
                workers_started_at_cut,
            );
            if diagnostics.next_pass_count() > next_pass_at_cut {
                break diagnostics;
            }
            assert!(
                Instant::now() < deadline,
                "Phase 96 next lane did not consume the execution-ready wake: \
                 scheduler={diagnostics:?}, probes={}, issues={}, pool={:?}, ring={ring:?}, \
                 cut=(wake={}, next_pass={}, source_reads={}, candidate_reads={}, unavailable={}, \
                 capacity_waits={}, flight_waits={}, workers_started={})",
                probes.load(Ordering::SeqCst),
                issues.load(Ordering::SeqCst),
                service.worker_pool_diagnostics(),
                wake_count_at_cut,
                next_pass_at_cut,
                next_source_reads_at_cut,
                next_candidate_reads_at_cut,
                next_unavailable_at_cut,
                next_capacity_waits_at_cut,
                next_flight_waits_at_cut,
                workers_started_at_cut,
            );
            std::thread::yield_now();
        }
    };
    assert!(scheduler_after_wake.wake_count() > wake_count_at_cut);
    assert!(scheduler_after_wake.next_pass_count() > next_pass_at_cut);
    assert!(!scheduler_after_wake.stopped());
    assert!(!scheduler_after_wake.fatal());

    let scheduler_after_issue = {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            let probe_count = probes.load(Ordering::SeqCst);
            let issue_count = issues.load(Ordering::SeqCst);
            let ring = pause.diagnostics();
            if issue_count == 1
                && probe_count == 0
                && diagnostics.workers_started() == workers_started_at_cut + 1
            {
                break diagnostics;
            }
            assert!(
                Instant::now() < deadline,
                "Phase 96 provider issue or scheduler worker did not advance exactly once: \
                 scheduler={diagnostics:?}, probes={probe_count}, issues={issue_count}, \
                 pool={:?}, ring={ring:?}, cut=(wake={}, next_pass={}, source_reads={}, \
                 candidate_reads={}, unavailable={}, capacity_waits={}, flight_waits={}, \
                 workers_started={})",
                service.worker_pool_diagnostics(),
                wake_count_at_cut,
                next_pass_at_cut,
                next_source_reads_at_cut,
                next_candidate_reads_at_cut,
                next_unavailable_at_cut,
                next_capacity_waits_at_cut,
                next_flight_waits_at_cut,
                workers_started_at_cut,
            );
            std::thread::yield_now();
        }
    };
    assert_eq!(probes.load(Ordering::SeqCst), 0);
    assert_eq!(issues.load(Ordering::SeqCst), 1);
    assert_eq!(
        scheduler_after_issue.workers_started(),
        workers_started_at_cut + 1
    );
    assert_eq!(scheduler_after_issue.workers_active(), 1);

    let (accepted_before, gate_before) = {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let accepted = phase96_route_entry(home, storage, ids);
            let gate = storage
                .input_gate(home, ids.thread, point_limit())
                .unwrap()
                .expect("the scheduled input retains its gate");
            if accepted.effective_state() == AcceptedRouteEffectiveState::Promoted
                && matches!(gate.state(), InputGateState::PendingTurn(_))
            {
                break (accepted, gate);
            }
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            let ring = pause.diagnostics();
            assert!(
                Instant::now() < deadline,
                "Phase 96 issued worker did not establish Promoted/PendingTurn state: \
                 scheduler={diagnostics:?}, probes={}, issues={}, pool={:?}, ring={ring:?}, \
                 route={accepted:?}, gate={gate:?}, cut=(wake={}, next_pass={}, source_reads={}, \
                 candidate_reads={}, unavailable={}, capacity_waits={}, flight_waits={}, \
                 workers_started={})",
                probes.load(Ordering::SeqCst),
                issues.load(Ordering::SeqCst),
                service.worker_pool_diagnostics(),
                wake_count_at_cut,
                next_pass_at_cut,
                next_source_reads_at_cut,
                next_candidate_reads_at_cut,
                next_unavailable_at_cut,
                next_capacity_waits_at_cut,
                next_flight_waits_at_cut,
                workers_started_at_cut,
            );
            std::thread::yield_now();
        }
    };
    let queued = {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let ring = pause.diagnostics();
            if (ring.sends, ring.receives, ring.len) == (1, 0, 1) {
                break ring;
            }
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            assert!(
                Instant::now() < deadline,
                "Phase 96 promoted worker did not queue exactly one driver command: \
                 scheduler={diagnostics:?}, probes={}, issues={}, pool={:?}, ring={ring:?}, \
                 route={accepted_before:?}, gate={gate_before:?}, cut=(wake={}, next_pass={}, \
                 source_reads={}, candidate_reads={}, unavailable={}, capacity_waits={}, \
                 flight_waits={}, workers_started={})",
                probes.load(Ordering::SeqCst),
                issues.load(Ordering::SeqCst),
                service.worker_pool_diagnostics(),
                wake_count_at_cut,
                next_pass_at_cut,
                next_source_reads_at_cut,
                next_candidate_reads_at_cut,
                next_unavailable_at_cut,
                next_capacity_waits_at_cut,
                next_flight_waits_at_cut,
                workers_started_at_cut,
            );
            std::thread::yield_now();
        }
    };
    assert_eq!((queued.sends, queued.receives, queued.len), (1, 0, 1));
    let scope = [stable_identity];
    let registry_before = crate::cas_projection::connection::registry::recovery_audit(&scope)
        .unwrap()
        .into_observations();
    assert_eq!(registry_before.len(), 1);

    fail_home_through_live_command(&service, state, &faults);
    wait_until("the exact old command frontier to close", || {
        !service.command_authorizer.is_open()
    });
    let command_queue = pause.release();
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(registry_projection);
    wait_until("the Phase 96 scheduler worker to surrender and join", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        diagnostics.workers_active() == 0
            && diagnostics.workers_joined() == 1
            && service.persistent_failure_cut_snapshot().state()
                == PersistentFailureCutState::Finished
    });
    let drained = command_queue
        .diagnostics()
        .expect("the adopted stable driver retains its command ring");
    assert_eq!((drained.sends, drained.receives, drained.len), (1, 1, 0));

    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .into_observations(),
        registry_before
    );

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the rejected scheduled owner must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let sealed_counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(sealed_counts.complete_candidate_count(), 1);
    assert_eq!(sealed_counts.connection_count(), 1);
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    assert_eq!(quarantine.metadata().candidate_count(), 1);
    assert_eq!(quarantine.metadata().retained_connection_count(), 1);
    retained_home.recover_same_home().unwrap();
    let recovered_storage = SyndicStorage::reacquire(&retained_home).unwrap();
    let accepted_after_rejection = phase96_route_entry(&retained_home, recovered_storage, ids);
    let gate_after_rejection = recovered_storage
        .input_gate(&retained_home, ids.thread, point_limit())
        .unwrap()
        .expect("the recovered rejected command retains its pending-turn gate");
    assert_eq!(accepted_after_rejection, accepted_before);
    assert_eq!(gate_after_rejection, gate_before);
    let replacement_issues = Arc::new(AtomicUsize::new(0));
    let replacement_shutdowns = Arc::new(AtomicUsize::new(0));
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::clone(&retained_home),
        config,
        Box::new(Phase96ReplacementProvider {
            issues: Arc::clone(&replacement_issues),
            shutdowns: Arc::clone(&replacement_shutdowns),
        }),
    )
    .unwrap();
    let adopted = quarantine.adopt_unpublished_service(replacement).unwrap();
    assert!(adopted.startup_fence_is_closed_for_test());
    assert_eq!(adopted.metadata().connection_count(), 1);
    assert_eq!(adopted.metadata().candidate_count(), 1);
    assert_eq!(adopted.adopted_connection_count_for_test(), 1);
    assert_eq!(replacement_issues.load(Ordering::SeqCst), 0);
    assert_eq!(issues.load(Ordering::SeqCst), 1);
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    let after_adoption = command_queue
        .diagnostics()
        .expect("the adopted driver remains parked on the stable command ring");
    assert_eq!(
        (after_adoption.sends, after_adoption.receives, after_adoption.len),
        (1, 1, 0)
    );
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .into_observations(),
        registry_before
    );

    adopted.dispose_after_recovery_failure().unwrap();
    assert_eq!(replacement_issues.load(Ordering::SeqCst), 0);
    assert_eq!(replacement_shutdowns.load(Ordering::SeqCst), 1);
    drop(connection);
    server.assert_quiet_and_close();
    server.join();
    drop(retained_home);
    drop(_directory);
}
