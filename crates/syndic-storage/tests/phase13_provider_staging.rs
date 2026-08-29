use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
#[cfg(feature = "test-faults")]
use beryl_model::SyndicThreadId;
use beryl_model::{CasItemId, CasThreadId, CasTurnId, SyndicContentId, SyndicItemId, SyndicTurnId};
use syndic_storage::*;

#[cfg(feature = "test-faults")]
#[path = "phase13_provider_staging/faults.rs"]
mod faults;
#[path = "phase13_provider_staging/restart.rs"]
mod restart;
#[cfg(feature = "test-faults")]
#[path = "support/mod.rs"]
mod support;

fn source(item: &str) -> CasItemSource {
    CasItemSource::new(
        CasTurnSource::new(
            CasThreadId::new("provider-thread").unwrap(),
            CasTurnId::new("provider-turn").unwrap(),
        ),
        CasItemId::new(item).unwrap(),
    )
}

fn agent_value(text: impl Into<String>) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text.into()),
        phase: None,
        memory_citation: None,
    })
}

fn agent_start(item: &str, text: impl Into<String>) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new(item).unwrap(),
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(10),
            item: agent_value(text),
        },
    )
}

fn agent_delta(
    ordinal: ProviderFrameOrdinalV1,
    item: &str,
    text: impl Into<String>,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ordinal,
        CasItemId::new(item).unwrap(),
        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
            delta: ProviderTextV1::inline(text.into()),
        }),
    )
}

#[cfg(feature = "test-faults")]
fn agent_completion(
    ordinal: ProviderFrameOrdinalV1,
    item: &str,
    text: impl Into<String>,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ordinal,
        CasItemId::new(item).unwrap(),
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(20),
            item: agent_value(text),
        },
    )
}

fn prepare_first(frame: ProviderItemFrameV1, content_byte: u8) -> PreparedProviderFrame {
    prepare_first_for(
        SyndicItemId::from_bytes([2; 16]),
        SyndicTurnId::from_bytes([3; 16]),
        source(frame.item_id().as_str()),
        SourceEventSequence::FIRST,
        frame,
        content_byte,
    )
}

fn prepare_first_for(
    item: SyndicItemId,
    turn: SyndicTurnId,
    source: CasItemSource,
    source_event: SourceEventSequence,
    frame: ProviderItemFrameV1,
    content_byte: u8,
) -> PreparedProviderFrame {
    prepare_provider_frame(ProviderFramePreparationPlan::first(
        item,
        turn,
        source,
        source_event,
        SyndicContentId::from_bytes([content_byte; 16]),
        frame,
    ))
    .unwrap()
}

fn prepare_next(
    prior: SealedProviderFrameReference,
    source_event: u64,
    frame: ProviderItemFrameV1,
) -> PreparedProviderFrame {
    prepare_next_for(
        SyndicItemId::from_bytes([2; 16]),
        SyndicTurnId::from_bytes([3; 16]),
        prior,
        source(frame.item_id().as_str()),
        SourceEventSequence::new(source_event).unwrap(),
        frame,
    )
}

fn prepare_next_for(
    item: SyndicItemId,
    turn: SyndicTurnId,
    prior: SealedProviderFrameReference,
    source: CasItemSource,
    source_event: SourceEventSequence,
    frame: ProviderItemFrameV1,
) -> PreparedProviderFrame {
    prepare_provider_frame(ProviderFramePreparationPlan::subsequent(
        item,
        turn,
        source,
        source_event,
        prior,
        frame,
    ))
    .unwrap()
}

fn stage_collect(
    prepared: &PreparedProviderFrame,
) -> (ProviderItemBuildRecord, Vec<ProviderFrameStageBatch>) {
    let home = restart::TestHome::new("stage-collect");
    let mut store = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    match store.execute_current(storage.current_begin_provider_frame_build(prepared)) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean provider-frame build begin, got {outcome:?}"),
    }
    let mut batches = Vec::new();
    let final_build = match stage_provider_frame(
        prepared,
        prepared.initial_build().clone(),
        &mut |batch: &ProviderFrameStageBatch| {
            batches.push(batch.clone());
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(
                    storage.stage_provider_frame_batch(
                        storage.revision(&store).unwrap(),
                        batch.clone(),
                    ),
                )
                .unwrap();
            store.execute(command)
        },
    )
    .unwrap()
    {
        ProviderFrameStageOutcome::Committed {
            value,
            receipt,
            later_failure: None,
        } => {
            let _receipt = receipt;
            value
        }
        ProviderFrameStageOutcome::Committed {
            receipt,
            later_failure: Some(failure),
            ..
        } => panic!(
            "expected clean provider-frame staging, got committed outcome with receipt {receipt:?} and later failure {failure:?}"
        ),
        ProviderFrameStageOutcome::NotCommitted { evidence } => {
            panic!("expected clean provider-frame staging, got definitive non-commit {evidence:?}")
        }
        ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected clean provider-frame staging, got indeterminate outcome {failure:?}")
        }
        ProviderFrameStageOutcome::Unchanged { value } => {
            panic!("expected provider-frame staging work, got unchanged build {value:?}")
        }
    };
    (final_build, batches)
}

#[cfg(feature = "test-faults")]
fn clean_command(outcome: CommandOutcome, operation: &str) {
    match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => {
            let _receipt = receipt;
        }
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(failure),
        } => panic!(
            "expected clean {operation}, got committed receipt {receipt:?} with later failure {failure:?}"
        ),
        CommandOutcome::NotCommitted { evidence } => {
            panic!("expected clean {operation}, got definitive non-commit {evidence:?}")
        }
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected clean {operation}, got indeterminate outcome {failure:?}")
        }
    }
}

#[cfg(feature = "test-faults")]
fn next_source_event(
    store: &HomeStore,
    storage: &SyndicStorage,
    turn: SyndicTurnId,
) -> SourceEventSequence {
    let state = storage
        .turn_state(store, turn, SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap_or_else(|| panic!("provider staging test turn disappeared"));
    SourceEventSequence::new(state.source_event_count() + 1).unwrap()
}

#[cfg(feature = "test-faults")]
fn converge_transcript(store: &HomeStore, storage: &SyndicStorage, thread: SyndicThreadId) {
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let thread_record = storage
        .thread(store, thread, limit)
        .unwrap()
        .unwrap_or_else(|| panic!("provider staging test thread disappeared"));
    let head = storage
        .transcript_view_head(store, thread, limit)
        .unwrap()
        .unwrap_or_else(|| panic!("provider staging test transcript head disappeared"));
    if head.lifecycle() == ProjectionLifecycle::Current {
        return;
    }
    let generation = head.generation();
    clean_command(
        store.execute_current(
            storage.current_start_transcript_build(StartTranscriptBuild::new(
                thread,
                thread_record.revision(),
                head.revision(),
            )),
        ),
        "transcript-build start",
    );
    for _ in 0..4_096 {
        let build = storage
            .transcript_build(store, thread, generation, limit)
            .unwrap()
            .unwrap_or_else(|| panic!("provider staging test transcript build disappeared"));
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        clean_command(
            store.execute_current(storage.current_advance_transcript_build(
                AdvanceTranscriptBuild::new(thread, generation, build.revision()),
            )),
            "transcript-build advance",
        );
    }
    panic!("bounded provider staging transcript rebuild did not finish");
}

#[cfg(feature = "test-faults")]
fn stage_and_publish(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    source: &CasTurnSource,
    prepared: &PreparedProviderFrame,
    publish: bool,
) -> (ProviderItemBuildRecord, Vec<ProviderFrameStageBatch>) {
    clean_command(
        store.execute_current(storage.current_begin_provider_frame_build(prepared)),
        "provider-frame build begin",
    );
    let mut batches = Vec::new();
    let mut build = match stage_provider_frame(
        prepared,
        prepared.initial_build().clone(),
        &mut |batch: &ProviderFrameStageBatch| {
            batches.push(batch.clone());
            store.execute_current(storage.current_stage_provider_frame_batch(batch.clone()))
        },
    )
    .unwrap()
    {
        ProviderFrameStageOutcome::Committed {
            value,
            receipt,
            later_failure: None,
        } => {
            let _receipt = receipt;
            value
        }
        ProviderFrameStageOutcome::Committed {
            receipt,
            later_failure: Some(failure),
            ..
        } => panic!(
            "expected clean provider-frame staging, got committed receipt {receipt:?} with later failure {failure:?}"
        ),
        ProviderFrameStageOutcome::NotCommitted { evidence } => {
            panic!("expected clean provider-frame staging, got definitive non-commit {evidence:?}")
        }
        ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected clean provider-frame staging, got indeterminate outcome {failure:?}")
        }
        ProviderFrameStageOutcome::Unchanged { value } => {
            panic!("expected provider-frame staging work, got unchanged build {value:?}")
        }
    };
    if !publish {
        return (build, batches);
    }
    for _ in 0..4_096 {
        if build.lifecycle() == ProviderItemBuildLifecycle::Sealed {
            break;
        }
        clean_command(
            store.execute_current(storage.current_compare_provider_completion(build)),
            "provider completion comparison",
        );
        build = storage
            .provider_item_build(
                store,
                prepared.initial_build().item_id(),
                SyndicPointReadLimit::new(1_000_000).unwrap(),
            )
            .unwrap()
            .unwrap_or_else(|| panic!("provider staging test build disappeared"));
    }
    assert_eq!(build.lifecycle(), ProviderItemBuildLifecycle::Sealed);

    let state = storage
        .turn_state(
            store,
            prepared.initial_build().turn_id(),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap_or_else(|| panic!("provider staging test turn disappeared"));
    let gate = storage
        .input_gate(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap_or_else(|| panic!("provider staging test input gate disappeared"));
    assert_eq!(
        prepared.initial_build().source_event().get(),
        state.source_event_count() + 1,
        "prepared provider frame must use the exact next source-event sequence"
    );
    clean_command(
        store.execute_current(
            storage.current_admit_live_source_event(
                LiveSourceEvent::new(
                    thread,
                    prepared.initial_build().turn_id(),
                    state.revision(),
                    gate.revision(),
                    prepared.initial_build().source_event(),
                    Some(source.clone()),
                    SourceEventPayload::ItemFrame {
                        item_id: prepared.initial_build().item_id(),
                        frame: Box::new(prepared.target().clone()),
                    },
                    SyndicTimestamp::from_unix_millis(100),
                )
                .unwrap(),
            ),
        ),
        "provider-frame live-source publication",
    );
    converge_transcript(store, &storage, thread);
    (build, batches)
}

#[test]
fn large_narrative_frame_uses_bounded_chunk_batches_and_one_selected_span() {
    let text = "x".repeat(CONTENT_CHUNK_MAX_BYTES * (CONTENT_APPEND_MAX_CHUNKS + 2));
    let prepared = prepare_first(agent_start("large", text.clone()), 4);
    let target_narrative = prepared.target().narrative().unwrap();

    assert_eq!(target_narrative.span_count(), 1);
    assert_eq!(target_narrative.logical_utf8_bytes(), text.len() as u64);
    let (final_build, batches) = stage_collect(&prepared);

    assert!(batches.len() >= 2);
    assert_eq!(final_build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
    assert_eq!(final_build.staged_narrative(), Some(target_narrative));
    assert_eq!(
        batches
            .iter()
            .map(|batch| batch.narrative_spans().len())
            .sum::<usize>(),
        1
    );
    let span = batches
        .iter()
        .flat_map(ProviderFrameStageBatch::narrative_spans)
        .next()
        .unwrap();
    assert_eq!(span.logical_start(), 0);
    assert_eq!(span.logical_end(), text.len() as u64);
    assert_eq!(
        span.frame_encoded_digest(),
        prepared.target().frame().encoded_digest()
    );
    for batch in &batches {
        assert!(!batch.chunks().is_empty() || !batch.narrative_spans().is_empty());
        assert!(batch.chunks().len() <= CONTENT_APPEND_MAX_CHUNKS);
        assert!(batch.narrative_spans().len() <= PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS);
    }
}

#[test]
fn operational_and_activity_spans_never_enter_selected_narrative() {
    let activity = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("activity").unwrap(),
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: ProviderItemV1::HookPrompt(ProviderHookPromptV1 {
                fragments: vec![ProviderHookPromptFragmentV1 {
                    text: ProviderTextV1::inline("activity"),
                    hook_run_id: ProviderTextV1::inline("hook"),
                }],
            }),
        },
    );
    let operational = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("operational").unwrap(),
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
                command: ProviderTextV1::inline("cargo check"),
                cwd: ProviderTextV1::inline("C:/workspace"),
                process_id: None,
                source: ProviderCommandSourceV1::Agent,
                status: ProviderCommandStatusV1::InProgress,
                command_actions: Vec::new(),
                aggregated_output: Some(ProviderTextV1::inline("checking")),
                exit_code: None,
                duration_ms: None,
            }),
        },
    );

    for (content_byte, frame) in [(5, activity), (6, operational)] {
        let prepared = prepare_first(frame, content_byte);
        assert!(prepared.target().frame().text_span_count() > 0);
        assert_eq!(prepared.target().narrative(), None);
        let (final_build, batches) = stage_collect(&prepared);
        assert_eq!(final_build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
        assert!(
            batches
                .iter()
                .all(|batch| batch.narrative_spans().is_empty())
        );
    }
}

#[cfg(feature = "test-faults")]
fn only_narrative_span(batches: &[ProviderFrameStageBatch]) -> ProviderNarrativeSpanRecord {
    let spans = batches
        .iter()
        .flat_map(ProviderFrameStageBatch::narrative_spans)
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 1);
    spans[0]
}

fn build_with_staged_narrative(
    initial: &ProviderItemBuildRecord,
    narrative: ProviderNarrativeReference,
) -> Result<ProviderItemBuildRecord, ProviderStorageRecordError> {
    ProviderItemBuildRecord::new(
        initial.item_id(),
        initial.turn_id(),
        initial.source().clone(),
        initial.source_event(),
        initial.revision(),
        initial.prior().cloned(),
        initial.target().clone(),
        initial.staged_chunk_count(),
        initial.staged_encoded_bytes(),
        initial.staged_chain_digest(),
        Some(narrative),
        initial.completion_check(),
        ProviderItemBuildLifecycle::Staging,
    )
}

#[cfg(feature = "test-faults")]
#[test]
fn empty_start_deltas_append_and_completion_preserves_narrative_pending_equality() {
    let home = restart::TestHome::new("narrative-lifecycle");
    let mut store = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    support::seed_populated(&store, storage.clone());

    let thread = support::id(40);
    let turn = support::populated::active_turn();
    let item = "narrative-life";
    let syndic_item = SyndicItemId::from_bytes([0xf1; 16]);
    let source = CasItemSource::new(
        CasTurnSource::new(
            support::populated::cas_thread(),
            support::populated::cas_turn(),
        ),
        CasItemId::new(item).unwrap(),
    );
    let first = prepare_first_for(
        syndic_item,
        turn,
        source.clone(),
        next_source_event(&store, &storage, turn),
        agent_start(item, ""),
        0xf1,
    );
    let (first_final, first_batches) =
        stage_and_publish(&store, &storage, thread, source.turn(), &first, true);
    let first_narrative = first_final.target().narrative().unwrap();
    assert_eq!(first_narrative.span_count(), 0);
    assert_eq!(first_narrative.logical_utf8_bytes(), 0);
    assert!(
        first_batches
            .iter()
            .all(|batch| batch.narrative_spans().is_empty())
    );

    let second_ordinal = ProviderFrameOrdinalV1::FIRST.checked_next().unwrap();
    let second = prepare_next_for(
        syndic_item,
        turn,
        first_final.target().clone(),
        source.clone(),
        next_source_event(&store, &storage, turn),
        agent_delta(second_ordinal, item, "Hello"),
    );
    let (second_final, second_batches) =
        stage_and_publish(&store, &storage, thread, source.turn(), &second, true);
    let second_narrative = second_final.target().narrative().unwrap();
    let hello = only_narrative_span(&second_batches);
    assert_eq!(
        second_narrative.generation(),
        ProviderNarrativeGeneration::FIRST
    );
    assert_eq!(second_narrative.span_count(), 1);
    assert_eq!(second_narrative.logical_utf8_bytes(), 5);
    assert_eq!(hello.logical_start(), 0);
    assert_eq!(hello.logical_end(), 5);
    assert_eq!(
        hello.resulting_chain_digest(),
        second_narrative.chain_digest()
    );

    let third_ordinal = second_ordinal.checked_next().unwrap();
    let third = prepare_next_for(
        syndic_item,
        turn,
        second_final.target().clone(),
        source.clone(),
        next_source_event(&store, &storage, turn),
        agent_delta(third_ordinal, item, " world"),
    );
    let (third_final, third_batches) =
        stage_and_publish(&store, &storage, thread, source.turn(), &third, true);
    let third_narrative = third_final.target().narrative().unwrap();
    let world = only_narrative_span(&third_batches);
    assert_eq!(third_narrative.generation(), second_narrative.generation());
    assert_eq!(third_narrative.span_count(), 2);
    assert_eq!(third_narrative.logical_utf8_bytes(), 11);
    assert_eq!(world.logical_start(), 5);
    assert_eq!(world.logical_end(), 11);
    assert_eq!(
        world,
        ProviderNarrativeSpanRecord::new(
            world.content_id(),
            world.generation(),
            world.logical_start(),
            world.logical_end(),
            world.frame_ordinal(),
            world.frame_encoded_digest(),
            world.source_start(),
            world.source_end(),
            world.source_digest(),
            second_narrative.chain_digest(),
        )
        .unwrap()
    );
    assert_eq!(
        world.resulting_chain_digest(),
        third_narrative.chain_digest()
    );

    let fourth_ordinal = third_ordinal.checked_next().unwrap();
    let completed = prepare_next_for(
        syndic_item,
        turn,
        third_final.target().clone(),
        source.clone(),
        next_source_event(&store, &storage, turn),
        agent_completion(fourth_ordinal, item, "Hello world"),
    );
    let (completed_final, completion_batches) =
        stage_and_publish(&store, &storage, thread, source.turn(), &completed, false);
    assert_eq!(completed_final.target().narrative(), Some(third_narrative));
    assert_eq!(
        completed_final.lifecycle(),
        ProviderItemBuildLifecycle::Staging
    );
    assert_eq!(
        completed_final.completion_check().unwrap().state(),
        ProviderNarrativeCompletionState::Pending(ProviderNarrativeComparisonFrontier::initial(
            third_narrative,
        ))
    );
    assert!(
        completion_batches
            .iter()
            .all(|batch| batch.narrative_spans().is_empty())
    );
    store.close().unwrap();
}

#[test]
fn malformed_narrative_chain_and_resume_frontier_are_rejected() {
    let item = "malformed-narrative";
    let first = prepare_first(agent_start(item, "Hello"), 8);
    let (first_final, _) = stage_collect(&first);
    let prior_narrative = first_final.target().narrative().unwrap();
    let ordinal = ProviderFrameOrdinalV1::FIRST.checked_next().unwrap();
    let delta = prepare_next(
        first_final.target().clone(),
        2,
        agent_delta(ordinal, item, " world"),
    );
    let initial = delta.initial_build();
    let target_narrative = delta.target().narrative().unwrap();

    let impossible_partial = ProviderNarrativeReference::new(
        target_narrative.content_id(),
        target_narrative.generation(),
        target_narrative.span_count(),
        prior_narrative.logical_utf8_bytes() + 1,
        [0x5a; 32],
    )
    .unwrap();
    let malformed_resume = build_with_staged_narrative(initial, impossible_partial).unwrap();
    let resume_error = stage_provider_frame(
        &delta,
        malformed_resume,
        &mut |_batch: &ProviderFrameStageBatch| {
            unreachable!("malformed resume must reject before staging")
        },
    )
    .unwrap_err();
    assert!(matches!(
        resume_error,
        ProviderFrameStageError::ResumeNarrativeFrontierMismatch
    ));

    let wrong_target_chain = ProviderNarrativeReference::new(
        target_narrative.content_id(),
        target_narrative.generation(),
        target_narrative.span_count(),
        target_narrative.logical_utf8_bytes(),
        [0xa5; 32],
    )
    .unwrap();
    let chain_error = build_with_staged_narrative(initial, wrong_target_chain).unwrap_err();
    assert!(matches!(
        chain_error,
        ProviderStorageRecordError::StagedNarrativeChainDigestMismatch
    ));
}

#[test]
fn batch_state_classification_and_uncommitted_expected_retry_are_exact() {
    let text = "b".repeat(CONTENT_CHUNK_MAX_BYTES * (CONTENT_APPEND_MAX_CHUNKS + 2));
    let prepared = prepare_first(agent_start("batch-state", text), 10);
    let home = restart::TestHome::new("batch-state");
    let mut store = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let stale_home_revision = store.home_revision().unwrap();
    match store.execute_current(storage.current_begin_provider_frame_build(&prepared)) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean provider-frame build begin, got {outcome:?}"),
    }
    let mut offered = None;
    let rejected =
        stage_provider_frame(
            &prepared,
            prepared.initial_build().clone(),
            &mut |batch: &ProviderFrameStageBatch| {
                offered = Some(batch.clone());
                let mut command = HomeCommand::new(stale_home_revision);
                command
                    .add(storage.stage_provider_frame_batch(
                        storage.revision(&store).unwrap(),
                        batch.clone(),
                    ))
                    .unwrap();
                store.execute(command)
            },
        )
        .unwrap();
    match rejected {
        ProviderFrameStageOutcome::NotCommitted { .. } => {}
        ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected stale command rejection, got indeterminate outcome {failure:?}")
        }
        ProviderFrameStageOutcome::Committed {
            receipt,
            later_failure,
            ..
        } => panic!(
            "expected stale command rejection, got committed outcome with receipt {receipt:?} and later failure {later_failure:?}"
        ),
        ProviderFrameStageOutcome::Unchanged { value } => {
            panic!("expected stale command rejection, got unchanged build {value:?}")
        }
    }

    let offered = offered.unwrap();
    assert_eq!(
        offered.classify_current(offered.expected_build()),
        ProviderFrameStageBatchState::Expected
    );
    assert_eq!(
        offered.classify_current(offered.next_build()),
        ProviderFrameStageBatchState::Next
    );
    let next = offered.next_build();
    assert_eq!(next.lifecycle(), ProviderItemBuildLifecycle::Staging);
    let conflict = next
        .advance(
            next.staged_chunk_count(),
            next.staged_encoded_bytes(),
            next.staged_chain_digest(),
            next.staged_narrative(),
            ProviderItemBuildLifecycle::Staging,
        )
        .unwrap();
    assert_eq!(
        offered.classify_current(&conflict),
        ProviderFrameStageBatchState::Conflict
    );

    let mut retried_first = None;
    let sealed =
        stage_provider_frame(
            &prepared,
            offered.expected_build().clone(),
            &mut |batch: &ProviderFrameStageBatch| {
                retried_first.get_or_insert_with(|| batch.clone());
                let mut command = HomeCommand::new(store.home_revision().unwrap());
                command
                    .add(storage.stage_provider_frame_batch(
                        storage.revision(&store).unwrap(),
                        batch.clone(),
                    ))
                    .unwrap();
                store.execute(command)
            },
        )
        .unwrap();
    let sealed = match sealed {
        ProviderFrameStageOutcome::Committed {
            value,
            receipt,
            later_failure: None,
        } => {
            let _receipt = receipt;
            value
        }
        ProviderFrameStageOutcome::Committed {
            receipt,
            later_failure: Some(failure),
            ..
        } => panic!(
            "expected clean retry staging outcome, got committed receipt {receipt:?} with later failure {failure:?}"
        ),
        ProviderFrameStageOutcome::NotCommitted { evidence } => {
            panic!("expected clean retry staging outcome, got definitive non-commit {evidence:?}")
        }
        ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected clean retry staging outcome, got indeterminate outcome {failure:?}")
        }
        ProviderFrameStageOutcome::Unchanged { value } => {
            panic!("expected clean retry staging outcome, got unchanged build {value:?}")
        }
    };
    assert_eq!(retried_first.as_ref(), Some(&offered));
    assert_eq!(sealed.lifecycle(), ProviderItemBuildLifecycle::Sealed);
}
