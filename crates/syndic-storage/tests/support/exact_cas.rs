#![allow(dead_code)]

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasItemId, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId,
    DiscussionContextOwnerId, DraftRevision, ExecutionBinding, PathFlavor, ProjectionRevision,
    RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SealedAssetReferenceSetProof,
    SyndicContentId, SyndicDraftId, SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::test_faults::{FixtureDelete, FixtureRecord};
use syndic_storage::{
    ActivateBinding, ActivityQueryHeadRecord, ActivityQuerySource, ActivityQuerySourceRecord,
    AdvanceItemProjectionBuild, AdvanceTranscriptBuild, BindingLifecycle, BindingRecord,
    BindingState, CanonicalItemPresentation, CanonicalItemRecord, CasItemSource, CasLineageProof,
    CasRepresentedPrefixProof, CasTurnSource, CompleteTerminalHistory, ComposerAtom,
    ComposerPayload, ContentEncoding, ContentLifecycle, ContentReference, ContextEnvelopeRecord,
    DraftByThreadRecord, DraftRecord, DraftSubmissionIntent, FinalizeNextTurnItem,
    FreezeNextTurnItem, GeneratedMediaResourceDisposition, HistorySummaryRecord,
    ImageLabelFrontier, ImageLabelOrdinal, ImageLabelOriginOwner, ImageLabelOriginSpanRecord,
    InputGateRecord, InputGateState, ItemProjectionGeneration, LiveSourceEvent, NativeCasLineage,
    PreparedContent, ProjectionLifecycle, ProviderFrameOrdinalV1, ProviderFramePreparationPlan,
    ProviderFrameStageOutcome, ProviderItemBuildLifecycle, ProviderItemFrameV1,
    ProviderItemLifecycle, ProviderItemObservationV1, ProviderItemV1,
    ProviderLifecycleTimestampMsV1, ProviderSubmittedContentV1, ProviderUserMessageV1,
    PublishActiveCasTurn, PublishValidBinding, ResourceBacking, SealedProviderFrameReference,
    SelectedPathProof, SourceEventPayload, SourceEventSequence, StartItemProjectionBuild,
    StartTranscriptBuild, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    ThreadImageLabelFrontiers, ThreadParentIndexRecord, ThreadRecord, TranscriptBuildPhase,
    TranscriptViewHeadRecord, TurnChildIndexRecord, TurnDepth, TurnItemIndexRecord,
    TurnItemOrdinal, TurnKind, TurnLifecycle, TurnStateRecord, TurnStateRevision,
    child_turn_chain_digest, empty_selected_path_digest, prepare_provider_frame,
    root_turn_chain_digest, stage_provider_frame,
};

use super::{batch, commit, prepared_content_records, seed_detached_canonical_draft_backing};

const CONVERGENCE_LIMIT: usize = 4_096;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
    operation: &str,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::Committed {
            later_failure: Some(failure),
            ..
        } => panic!("exact-CAS {operation} committed with a later failure: {failure:?}"),
        CommandOutcome::NotCommitted { evidence } => {
            panic!("exact-CAS {operation} did not commit: {evidence:?}")
        }
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("exact-CAS {operation} was indeterminate: {failure:?}")
        }
    }
}

pub fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-test-root-history",
        )
        .unwrap(),
    )
}

pub fn tool_profile() -> CasConversationToolProfile {
    CasConversationToolProfile::v1([248; 32])
}

pub fn submit_current_draft(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    next_draft_id: SyndicDraftId,
    submitted_item_id: SyndicItemId,
    text: &str,
    submitted_at: SyndicTimestamp,
) -> SyndicTurnId {
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    submit_prepared_current_draft(
        store,
        storage,
        thread_id,
        next_draft_id,
        submitted_item_id,
        &content,
        None,
        submitted_at,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn submit_prepared_current_draft(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    next_draft_id: SyndicDraftId,
    submitted_item_id: SyndicItemId,
    prepared_content: &PreparedContent,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
    submitted_at: SyndicTimestamp,
) -> SyndicTurnId {
    let thread = storage
        .thread(store, thread_id, point_limit())
        .unwrap()
        .expect("exact-CAS submission thread exists");
    let current = storage
        .current_draft(store, thread_id, point_limit())
        .unwrap()
        .expect("exact-CAS submission draft exists");
    assert_ne!(current.draft().id(), next_draft_id);
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .expect("exact-CAS submission gate exists");
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert_eq!(gate.live_count(), 0);
    let transcript = storage
        .transcript_view_head(store, thread_id, point_limit())
        .unwrap()
        .expect("exact-CAS submission transcript head exists");
    let history = storage
        .history_summary(store, thread_id, point_limit())
        .unwrap()
        .expect("exact-CAS submission history summary exists");
    let activity = storage
        .activity_query_head(store, thread_id, point_limit())
        .unwrap()
        .expect("exact-CAS submission activity head exists");
    assert!(!activity.source_active());
    assert_eq!(activity.logical_row_count(), activity.completed_row_count());
    let binding = storage
        .current_binding(store, thread_id, point_limit())
        .unwrap()
        .expect("exact-CAS submission binding exists");

    let turn_id = current.draft().id().submitted_turn_id();
    let mut context_transition = None;
    let (parent, next_context_owner) = match current.draft().submission_intent() {
        DraftSubmissionIntent::Ordinary => (thread.committed_tail(), thread.context_owner_id()),
        DraftSubmissionIntent::Replacement(intent) => {
            assert_eq!(intent.selected_path(), thread.selected_path());
            let target = storage
                .turn(store, intent.target_turn_id(), point_limit())
                .unwrap()
                .expect("exact-CAS replacement target exists");
            assert_eq!(target.kind(), TurnKind::OrdinaryUser);
            (target.parent().turn(), thread.context_owner_id())
        }
        DraftSubmissionIntent::DiscussionContext(owner) => {
            assert_eq!(owner, DiscussionContextOwnerId::Draft(current.draft().id()));
            assert_eq!(thread.context_owner_id(), Some(owner));
            let envelope = storage
                .context_envelope(store, owner, point_limit())
                .unwrap()
                .expect("exact-CAS discussion-context envelope exists");
            let submitted_owner = DiscussionContextOwnerId::SubmittedTurn(turn_id);
            let source_turn = envelope.envelope().descriptor().source().turn_id();
            context_transition = Some((
                owner,
                ContextEnvelopeRecord::new(
                    submitted_owner,
                    envelope.revision(),
                    envelope.envelope().clone(),
                ),
            ));
            (Some(source_turn), Some(submitted_owner))
        }
    };
    let (parent_kind, depth, digest, ancestor_skip) = match parent {
        Some(parent_id) => {
            let parent_record = storage
                .turn(store, parent_id, point_limit())
                .unwrap()
                .expect("exact-CAS submission parent exists");
            let parent_state = storage
                .turn_state(store, parent_id, point_limit())
                .unwrap()
                .expect("exact-CAS submission parent state exists");
            assert!(parent_state.lifecycle().is_proven_terminal());
            let depth = parent_record.depth().checked_next().unwrap();
            let ancestor_skip = child_ancestor_skip(store, storage, parent_record.clone(), depth);
            (
                syndic_storage::ConversationParent::Turn(parent_id),
                depth,
                child_turn_chain_digest(turn_id, parent_id, parent_record.chain_digest()),
                Some(ancestor_skip),
            )
        }
        None => (
            syndic_storage::ConversationParent::Root,
            TurnDepth::FIRST,
            root_turn_chain_digest(turn_id),
            None,
        ),
    };
    let thread_revision = thread.revision().checked_next().unwrap();
    let selected = SelectedPathProof::new(Some(turn_id), thread_revision, digest);
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let (content, content_records) = prepared_content_records(prepared_content);
    assert_eq!(content.encoding(), ContentEncoding::ComposerV1);
    validate_asset_reference_set(content, asset_reference_set);
    let (image_label_frontiers, image_label_origin) =
        submission_image_label_authority(&thread, content, asset_reference_set, submitted_item_id);

    let staging_thread = detached_staging_thread(thread_id, next_draft_id);
    assert!(
        storage
            .thread(store, staging_thread, point_limit())
            .unwrap()
            .is_none(),
        "exact-CAS detached draft staging identity is occupied"
    );
    let next_root_history =
        seed_detached_canonical_draft_backing(store, storage, staging_thread, next_draft_id);

    let next_activity_period = if activity.source().is_none() {
        activity.work_period()
    } else {
        activity.work_period().checked_next().unwrap()
    };
    let activity_source = ActivityQuerySource::new(thread_id, turn_id);
    let activity_head = ActivityQueryHeadRecord::new(
        thread_id,
        next_activity_period,
        Some(activity_source),
        true,
        0,
        activity.revision().checked_next().unwrap(),
        1,
        0,
        0,
        0,
        0,
        None,
        ProjectionLifecycle::Current,
    )
    .unwrap();
    let binding_revision = binding.binding().revision().checked_next().unwrap();
    let next_draft_revision = DraftRevision::new(1).unwrap();
    let next_thread = ThreadRecord::new(
        thread_id,
        selected,
        next_draft_id,
        thread.lineage(),
        image_label_frontiers,
        next_context_owner,
    );
    let next_history = HistorySummaryRecord::new(
        thread_id,
        history.revision().checked_next().unwrap(),
        thread_revision,
        Some(turn_id),
        digest,
        false,
        submitted_at,
    );
    let mut records = content_records;
    records.extend([
        FixtureRecord::Thread(next_thread),
        FixtureRecord::Draft(DraftRecord::new(
            next_draft_id,
            thread_id,
            next_draft_revision,
            DraftSubmissionIntent::Ordinary,
            next_root_history,
            submitted_at,
            submitted_at,
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread_id,
            next_draft_id,
            next_draft_revision,
            thread_revision,
        )),
        FixtureRecord::Turn(syndic_storage::TurnRecord::new(
            turn_id,
            thread_id,
            TurnKind::OrdinaryUser,
            parent_kind,
            ancestor_skip,
            depth,
            digest,
            submitted_at,
        )),
        FixtureRecord::TurnState(
            TurnStateRecord::with_capture_frontiers(
                turn_id,
                TurnStateRevision::FIRST,
                TurnLifecycle::Pending,
                0,
                1,
                0,
                1,
                0,
                None,
                submitted_at,
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            submitted_item_id,
            turn_id,
            TurnItemOrdinal::FIRST,
            projection_revision,
            content,
            asset_reference_set,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn_id,
            TurnItemOrdinal::FIRST,
            submitted_item_id,
            projection_revision,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread_id,
                gate.revision().checked_next().unwrap(),
                InputGateState::PendingTurn(turn_id),
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread_id,
            transcript.generation().checked_next().unwrap(),
            transcript.revision().checked_next().unwrap(),
            0,
            Some(turn_id),
            digest,
            ProjectionLifecycle::Stale,
        )),
        FixtureRecord::HistorySummary(next_history),
        FixtureRecord::ActivityQueryHead(activity_head),
        FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
            thread_id,
            next_activity_period,
            activity_source,
            None,
            0,
            true,
            None,
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread_id,
            binding_revision,
            selected,
            BindingState::unbound("exact-CAS pending turn awaits projection").unwrap(),
        )),
        FixtureRecord::BindingHead(syndic_storage::BindingHeadRecord::new(
            thread_id,
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        )),
    ]);
    if let Some(parent_id) = parent {
        records.push(FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            parent_id, turn_id, depth, digest,
        )));
    }
    if let (Some(parent_thread_id), Some(context_owner_id)) =
        (thread.parent_thread_id(), next_context_owner)
    {
        records.push(FixtureRecord::ThreadParent(ThreadParentIndexRecord::new(
            parent_thread_id,
            thread_id,
            thread_revision,
            context_owner_id,
        )));
    }
    if let Some((_, envelope)) = context_transition.as_ref() {
        records.push(FixtureRecord::ContextEnvelope(envelope.clone()));
    }
    if let Some(origin) = image_label_origin {
        records.push(FixtureRecord::ImageLabelOriginSpan(origin));
    }
    let mut fixture = batch(records);
    fixture
        .delete(FixtureDelete::Draft(current.draft().id()))
        .unwrap();
    if let Some((owner, _)) = context_transition {
        fixture
            .delete(FixtureDelete::ContextEnvelope(owner))
            .unwrap();
    }
    commit(store, storage, fixture);
    turn_id
}

fn validate_asset_reference_set(
    content: ContentReference,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
) {
    let marker_count = content.summary().image_marker_count();
    match (marker_count, asset_reference_set) {
        (0, None) => {}
        (0, Some(_)) | (_, None) => {
            panic!("exact-CAS content and asset-reference proof disagree")
        }
        (_, Some(proof)) => assert_eq!(
            proof.sequential(),
            content.sealed_marker_summary().unwrap().sequential(),
            "exact-CAS asset-reference proof source disagrees with content"
        ),
    }
}

fn submission_image_label_authority(
    thread: &ThreadRecord,
    content: ContentReference,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
    submitted_item_id: SyndicItemId,
) -> (
    ThreadImageLabelFrontiers,
    Option<ImageLabelOriginSpanRecord>,
) {
    validate_asset_reference_set(content, asset_reference_set);
    let frontiers = thread.image_label_frontiers();
    let Some(proof) = asset_reference_set else {
        return (frontiers, None);
    };
    let end = proof
        .sequential()
        .maximum_image_label()
        .expect("exact-CAS marker-bearing content has a maximum label");
    if frontiers.current().contains(end) {
        return (frontiers, None);
    }
    let start = ImageLabelOrdinal::new(frontiers.current().get().checked_add(1).unwrap()).unwrap();
    let origin = ImageLabelOriginSpanRecord::new(
        thread.id(),
        start,
        end,
        ImageLabelOriginOwner::CanonicalItem(submitted_item_id),
        proof,
    )
    .unwrap();
    let advanced = ThreadImageLabelFrontiers::new(
        frontiers.inherited(),
        ImageLabelFrontier::from_raw(end.get()),
    )
    .unwrap();
    (advanced, Some(origin))
}

fn detached_staging_thread(owner: SyndicThreadId, next_draft: SyndicDraftId) -> SyndicThreadId {
    let mut bytes = *next_draft.as_bytes();
    for byte in &mut bytes {
        *byte ^= 0x5a;
    }
    let mut staging = SyndicThreadId::from_bytes(bytes);
    if staging == owner {
        bytes[0] ^= 1;
        staging = SyndicThreadId::from_bytes(bytes);
    }
    staging
}

fn child_ancestor_skip(
    store: &HomeStore,
    storage: SyndicStorage,
    mut current: syndic_storage::TurnRecord,
    child_depth: TurnDepth,
) -> SyndicTurnId {
    let target_depth = (child_depth.get() & (child_depth.get() - 1)).max(1);
    for _ in 0..2_080 {
        if current.depth().get() == target_depth {
            return current.id();
        }
        let skip_depth = (current.depth().get() & (current.depth().get() - 1)).max(1);
        let next = if skip_depth >= target_depth {
            current
                .ancestor_skip()
                .expect("exact-CAS non-root ancestor skip exists")
        } else {
            current
                .parent()
                .turn()
                .expect("exact-CAS ancestry reaches target")
        };
        current = storage
            .turn(store, next, point_limit())
            .unwrap()
            .expect("exact-CAS ancestor exists");
    }
    panic!("exact-CAS ancestry exceeded its fixed bound")
}

fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(1).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

pub fn establish_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    started_at: SyndicTimestamp,
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
    let prior = current
        .binding()
        .revision()
        .get()
        .checked_sub(1)
        .and_then(|revision| BindingRevision::new(revision).ok())
        .and_then(|revision| {
            storage
                .binding(store, thread, revision, point_limit())
                .unwrap()
        });
    let (execution, cas_thread, lineage, native_turn_count, profile) =
        match prior.as_ref().map(|record| record.state()) {
            Some(BindingState::Valid(usable))
                if usable.represented_prefix().tail() == represented.tail()
                    && usable.represented_prefix().digest() == represented.digest() =>
            {
                (
                    usable.execution().clone(),
                    usable.cas_thread_id().clone(),
                    usable.lineage(),
                    usable.native_turn_count(),
                    usable.tool_profile(),
                )
            }
            _ => {
                let mechanism = if represented.tail().is_some() {
                    NativeCasLineage::Fork
                } else {
                    NativeCasLineage::Fresh
                };
                let execution = storage
                    .thread_execution(store, thread, point_limit())
                    .unwrap()
                    .expect("exact-CAS thread retains execution")
                    .execution()
                    .clone();
                (
                    execution,
                    CasThreadId::new(format!("test-thread-{turn}")).unwrap(),
                    CasLineageProof::native(mechanism, represented).unwrap(),
                    CasNativeTurnCount::ZERO,
                    tool_profile(),
                )
            }
        };
    execute(
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
                native_turn_count,
                profile,
                lineage,
            ),
        ),
        "valid-binding publication",
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
    execute(
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
                loaded_generation(),
                started_at,
            ),
        ),
        "binding activation",
    );
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let cas_turn = CasTurnId::new(format!("test-turn-{turn}")).unwrap();
    execute(
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
                started_at,
            ),
        ),
        "active-CAS-turn publication",
    );
    CasTurnSource::new(cas_thread, cas_turn)
}

pub fn admit_event(
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
        SourceEventSequence::new(state.source_event_count().checked_add(1).unwrap()).unwrap(),
        Some(source.clone()),
        payload,
        observed_at,
    )
    .unwrap();
    execute(
        store,
        storage.admit_live_source_event(storage.revision(store).unwrap(), event),
        "live-source event admission",
    );
}

fn provider_content_id(item_id: SyndicItemId) -> SyndicContentId {
    let mut bytes = *item_id.as_bytes();
    for byte in &mut bytes {
        *byte ^= 0xa5;
    }
    SyndicContentId::from_bytes(bytes)
}

#[allow(clippy::too_many_arguments)]
pub fn admit_item_frame(
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
    let source_event =
        SourceEventSequence::new(state.source_event_count().checked_add(1).unwrap()).unwrap();
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
            provider_content_id(item_id),
            frame,
        ),
    };
    let prepared = prepare_provider_frame(plan).unwrap();
    execute(
        store,
        storage.begin_provider_frame_build(storage.revision(store).unwrap(), &prepared),
        "provider-frame build begin",
    );
    let mut build =
        match stage_provider_frame(
            &prepared,
            prepared.initial_build().clone(),
            &mut |stage: &syndic_storage::ProviderFrameStageBatch| {
                let mut command = HomeCommand::new(store.home_revision().unwrap());
                command
                    .add(storage.stage_provider_frame_batch(
                        storage.revision(store).unwrap(),
                        stage.clone(),
                    ))
                    .unwrap();
                store.execute(command)
            },
        )
        .unwrap()
        {
            ProviderFrameStageOutcome::Unchanged { value } => value,
            ProviderFrameStageOutcome::Committed {
                value,
                later_failure: None,
                ..
            } => value,
            ProviderFrameStageOutcome::Committed {
                later_failure: Some(failure),
                ..
            } => panic!("provider-frame staging committed with a later failure: {failure:?}"),
            ProviderFrameStageOutcome::NotCommitted { evidence } => {
                panic!("provider-frame staging did not commit: {evidence:?}")
            }
            ProviderFrameStageOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                reconciliation.install();
                panic!("provider-frame staging was indeterminate: {failure:?}")
            }
        };
    for _ in 0..CONVERGENCE_LIMIT {
        if build.lifecycle() == ProviderItemBuildLifecycle::Sealed {
            let sealed = prepared.target().clone();
            assert_eq!(build.target(), &sealed);
            admit_event(
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
        execute(
            store,
            storage.compare_provider_completion(storage.revision(store).unwrap(), build),
            "provider-frame completion comparison",
        );
        build = storage
            .provider_item_build(store, item_id, point_limit())
            .unwrap()
            .unwrap()
            .clone();
    }
    panic!("bounded provider-frame completion did not converge")
}

#[allow(clippy::too_many_arguments)]
pub fn admit_started_then_completed_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    cas_item: CasItemId,
    started: ProviderItemV1,
    completed: ProviderItemV1,
    started_at: SyndicTimestamp,
    completed_at: SyndicTimestamp,
) {
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        item_id,
        source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(started_at.unix_millis()),
                item: started,
            },
        ),
        started_at,
    );
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        item_id,
        source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(completed_at.unix_millis()),
                item: completed,
            },
        ),
        completed_at,
    );
}

pub fn correlate_user_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    observed_at: SyndicTimestamp,
) {
    let item = storage
        .canonical_item(store, item_id, point_limit())
        .unwrap()
        .unwrap();
    let content = item
        .presentation_content()
        .expect("submitted user fixture has sealed composer content");
    let cas_item = CasItemId::new(format!("test-user-{item_id}")).unwrap();
    let provider_item = ProviderItemV1::UserMessage(ProviderUserMessageV1 {
        client_id: None,
        submitted: ProviderSubmittedContentV1 { content },
    });
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        item_id,
        source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(observed_at.unix_millis()),
                item: provider_item.clone(),
            },
        ),
        observed_at,
    );
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        item_id,
        source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(observed_at.unix_millis()),
                item: provider_item,
            },
        ),
        observed_at,
    );
}

pub fn converge_and_release_terminal_history(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
) {
    converge_items(store, storage, thread_id, turn_id);
    converge_transcript(store, storage, thread_id);
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(store, turn_id, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.complete_terminal_history(
            storage.revision(store).unwrap(),
            CompleteTerminalHistory::new(
                thread_id,
                turn_id,
                gate,
                state.revision(),
                head.generation(),
                head.revision(),
            ),
        ),
        "terminal-history release",
    );
}

fn converge_items(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
) {
    for _ in 0..CONVERGENCE_LIMIT {
        let state = storage
            .turn_state(store, turn_id, point_limit())
            .unwrap()
            .unwrap();
        if state.finalized_item_count() == state.item_count() {
            return;
        }
        let ordinal =
            TurnItemOrdinal::new(state.finalized_item_count().checked_add(1).unwrap()).unwrap();
        let after = ordinal
            .get()
            .checked_sub(1)
            .and_then(|value| TurnItemOrdinal::new(value).ok());
        let page = storage
            .turn_items(
                store,
                turn_id,
                after,
                CursorReadLimits::new(1, 1_000_000).unwrap(),
            )
            .unwrap();
        let index = page.records().first().expect("next terminal item exists");
        assert_eq!(index.ordinal(), ordinal);
        let item = storage
            .canonical_item(store, index.item_id(), point_limit())
            .unwrap()
            .unwrap();
        if item.provider_lifecycle() != ProviderItemLifecycle::Completed {
            return;
        }
        if let Some(content) = item.provider_content() {
            let manifest = storage
                .content_manifest(store, content.id(), point_limit())
                .unwrap()
                .unwrap();
            if manifest.lifecycle() == ContentLifecycle::Live {
                execute(
                    store,
                    storage.freeze_next_turn_item(
                        storage.revision(store).unwrap(),
                        FreezeNextTurnItem::new(
                            thread_id,
                            turn_id,
                            state.revision(),
                            ordinal,
                            item.id(),
                            state.updated_at(),
                        ),
                    ),
                    "terminal item freeze",
                );
            }
        }
        if generated_media_is_waiting(store, storage, &item) {
            return;
        }
        project_item_if_needed(store, storage, item.id());
        let state = storage
            .turn_state(store, turn_id, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.finalize_next_turn_item(
                storage.revision(store).unwrap(),
                FinalizeNextTurnItem::new(
                    thread_id,
                    turn_id,
                    state.revision(),
                    ordinal,
                    item.id(),
                    state.updated_at(),
                ),
            ),
            "terminal item finalization",
        );
    }
    panic!("bounded terminal item convergence did not finish")
}

fn generated_media_is_waiting(
    store: &HomeStore,
    storage: SyndicStorage,
    item: &CanonicalItemRecord,
) -> bool {
    let CanonicalItemPresentation::GeneratedMedia { resource_id } = item.presentation() else {
        return false;
    };
    let resource = storage
        .resource(store, *resource_id, point_limit())
        .unwrap()
        .unwrap();
    matches!(
        resource.backing(),
        ResourceBacking::GeneratedMedia(
            GeneratedMediaResourceDisposition::PendingAsset
                | GeneratedMediaResourceDisposition::Unavailable(_)
        )
    )
}

fn project_item_if_needed(store: &HomeStore, storage: SyndicStorage, item_id: SyndicItemId) {
    let item = storage
        .canonical_item(store, item_id, point_limit())
        .unwrap()
        .unwrap();
    if item.projection_source().is_none() {
        return;
    }
    let head = storage
        .item_projection_head(store, item_id, point_limit())
        .unwrap();
    if head
        .as_ref()
        .is_some_and(|head| head.lifecycle() == ProjectionLifecycle::Current)
    {
        return;
    }
    let generation = head
        .as_ref()
        .map_or(ItemProjectionGeneration::FIRST, |head| {
            head.generation().checked_next().unwrap()
        });
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item_id, item.revision(), generation),
        ),
        "item-projection build start",
    );
    for _ in 0..CONVERGENCE_LIMIT {
        if storage
            .item_projection_head(store, item_id, point_limit())
            .unwrap()
            .as_ref()
            .is_some_and(|head| head.lifecycle() == ProjectionLifecycle::Current)
        {
            return;
        }
        let build = storage
            .item_projection_build(store, item_id, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item_id, generation, build.revision()),
            ),
            "item-projection build advance",
        );
    }
    panic!("bounded item-projection convergence did not finish")
}

fn converge_transcript(store: &HomeStore, storage: SyndicStorage, thread_id: SyndicThreadId) {
    let thread = storage
        .thread(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    if head.lifecycle() == ProjectionLifecycle::Current {
        return;
    }
    let generation = head.generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread_id, thread.revision(), head.revision()),
        ),
        "transcript build start",
    );
    for _ in 0..CONVERGENCE_LIMIT {
        let build = storage
            .transcript_build(store, thread_id, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread_id, generation, build.revision()),
            ),
            "transcript build advance",
        );
    }
    panic!("bounded transcript convergence did not finish")
}
