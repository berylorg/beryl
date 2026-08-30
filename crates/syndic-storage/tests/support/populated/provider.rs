use super::*;
use beryl_home_store::{CommandOutcome, CommitReceipt, HomeStore};
use beryl_model::SyndicThreadId;
use syndic_storage::SyndicStorage;

pub(super) struct ProviderItemFixture {
    pub(super) frames: Vec<SealedProviderFrameReference>,
    pub(super) canonical: SealedProviderFrameReference,
    prepared: Vec<PreparedProviderFrame>,
}

#[derive(Clone, Copy)]
pub(super) enum AgentItemFixtureState {
    Live,
    Completed,
    Finalized,
}

pub(super) fn agent_item_fixture(
    item: SyndicItemId,
    turn: SyndicTurnId,
    source: CasItemSource,
    first_source_event: SourceEventSequence,
    phase: ProviderMessagePhaseV1,
    text: &str,
    state: AgentItemFixtureState,
) -> ProviderItemFixture {
    let cas_item = source.item_id().clone();
    let mut ordinal = ProviderFrameOrdinalV1::FIRST;
    let mut sequence = first_source_event;
    let mut frames = vec![(
        sequence,
        ProviderItemFrameV1::new(
            ordinal,
            cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(1),
                item: agent_message("", phase),
            },
        ),
    )];
    if !text.is_empty() {
        ordinal = ordinal.checked_next().unwrap();
        sequence = next_source_event(sequence);
        frames.push((
            sequence,
            ProviderItemFrameV1::new(
                ordinal,
                cas_item.clone(),
                ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                    delta: ProviderTextV1::inline(text),
                }),
            ),
        ));
    }
    if !matches!(state, AgentItemFixtureState::Live) {
        ordinal = ordinal.checked_next().unwrap();
        sequence = next_source_event(sequence);
        frames.push((
            sequence,
            ProviderItemFrameV1::new(
                ordinal,
                cas_item,
                ProviderItemObservationV1::Completed {
                    observed_at: ProviderLifecycleTimestampMsV1::new(2),
                    item: agent_message(text, phase),
                },
            ),
        ));
    }
    provider_item_fixture(item, turn, source, frames)
}

pub(super) fn command_item_fixture(
    item: SyndicItemId,
    turn: SyndicTurnId,
    source: CasItemSource,
    first_source_event: SourceEventSequence,
) -> ProviderItemFixture {
    let cas_item = source.item_id().clone();
    let command =
        |status: ProviderCommandStatusV1, output: Option<&str>, exit_code: Option<i32>| {
            ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
                command: ProviderTextV1::inline("cargo check"),
                cwd: ProviderTextV1::inline("C:/workspace"),
                process_id: None,
                source: ProviderCommandSourceV1::Agent,
                status,
                command_actions: Vec::new(),
                aggregated_output: output.map(ProviderTextV1::inline),
                exit_code,
                duration_ms: exit_code.map(|_| 1_i64),
            })
        };
    provider_item_fixture(
        item,
        turn,
        source,
        vec![
            (
                first_source_event,
                ProviderItemFrameV1::new(
                    ProviderFrameOrdinalV1::FIRST,
                    cas_item.clone(),
                    ProviderItemObservationV1::Started {
                        observed_at: ProviderLifecycleTimestampMsV1::new(1),
                        item: command(ProviderCommandStatusV1::InProgress, None::<&str>, None),
                    },
                ),
            ),
            (
                next_source_event(first_source_event),
                ProviderItemFrameV1::new(
                    ProviderFrameOrdinalV1::new(2).unwrap(),
                    cas_item,
                    ProviderItemObservationV1::Completed {
                        observed_at: ProviderLifecycleTimestampMsV1::new(2),
                        item: command(ProviderCommandStatusV1::Completed, Some("ok"), Some(0)),
                    },
                ),
            ),
        ],
    )
}

fn agent_message(text: &str, phase: ProviderMessagePhaseV1) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: Some(phase),
        memory_citation: None,
    })
}

fn next_source_event(sequence: SourceEventSequence) -> SourceEventSequence {
    SourceEventSequence::new(sequence.get().checked_add(1).unwrap()).unwrap()
}

pub(super) fn correlated_user_item_fixture(
    item: SyndicItemId,
    turn: SyndicTurnId,
    source: CasItemSource,
    first_source_event: SourceEventSequence,
    content: ContentReference,
) -> ProviderItemFixture {
    let cas_item = source.item_id().clone();
    let submitted = ProviderItemV1::UserMessage(ProviderUserMessageV1 {
        client_id: None,
        submitted: ProviderSubmittedContentV1 { content },
    });
    let completed = submitted.clone();
    provider_item_fixture(
        item,
        turn,
        source,
        vec![
            (
                first_source_event,
                ProviderItemFrameV1::new(
                    ProviderFrameOrdinalV1::FIRST,
                    cas_item.clone(),
                    ProviderItemObservationV1::Started {
                        observed_at: ProviderLifecycleTimestampMsV1::new(1),
                        item: submitted,
                    },
                ),
            ),
            (
                next_source_event(first_source_event),
                ProviderItemFrameV1::new(
                    ProviderFrameOrdinalV1::FIRST.checked_next().unwrap(),
                    cas_item,
                    ProviderItemObservationV1::Completed {
                        observed_at: ProviderLifecycleTimestampMsV1::new(2),
                        item: completed,
                    },
                ),
            ),
        ],
    )
}

fn provider_item_fixture(
    item: SyndicItemId,
    turn: SyndicTurnId,
    source: CasItemSource,
    frames: Vec<(SourceEventSequence, ProviderItemFrameV1)>,
) -> ProviderItemFixture {
    let mut published = Vec::with_capacity(frames.len());
    let mut prepared_frames = Vec::with_capacity(frames.len());
    let mut prior = None;
    for (source_event, frame) in frames {
        let plan = match prior.clone() {
            Some(prior) => ProviderFramePreparationPlan::subsequent(
                item,
                turn,
                source.clone(),
                source_event,
                prior,
                frame,
            ),
            None => ProviderFramePreparationPlan::first(
                item,
                turn,
                source.clone(),
                source_event,
                SyndicContentId::from_bytes(*item.as_bytes()),
                frame,
            ),
        };
        let prepared = prepare_provider_frame(plan).unwrap();
        let target = prepared.target().clone();
        prior = Some(target.clone());
        published.push(target);
        prepared_frames.push(prepared);
    }
    let latest = published.last().cloned().unwrap();
    ProviderItemFixture {
        frames: published,
        canonical: latest,
        prepared: prepared_frames,
    }
}

pub(super) struct ProviderSeedTurn {
    pub(super) thread: SyndicThreadId,
    pub(super) turn: SyndicTurnId,
    pub(super) source: CasTurnSource,
    pub(super) state_revision: TurnStateRevision,
    pub(super) gate_revision: InputGateRevision,
    pub(super) observed_at: SyndicTimestamp,
}

impl ProviderItemFixture {
    pub(super) fn seed(
        &self,
        store: &HomeStore,
        storage: &SyndicStorage,
        turn: &mut ProviderSeedTurn,
        receipts: &mut Vec<CommitReceipt>,
    ) {
        for prepared in &self.prepared {
            accept_clean(
                store.execute_current(storage.current_begin_provider_frame_build(prepared)),
                "provider-frame build begin",
                receipts,
            );
            let mut build = match stage_provider_frame(
                prepared,
                prepared.initial_build().clone(),
                &mut |batch: &ProviderFrameStageBatch| {
                    store.execute_current(storage.current_stage_provider_frame_batch(batch.clone()))
                },
            )
            .unwrap_or_else(|error| panic!("provider-frame staging failed: {error}"))
            {
                ProviderFrameStageOutcome::Committed {
                    value,
                    receipt,
                    later_failure: None,
                } => {
                    receipts.push(receipt);
                    value
                }
                outcome => panic!("expected clean provider-frame staging, got {outcome:?}"),
            };
            for _ in 0..4_096 {
                if build.lifecycle() == ProviderItemBuildLifecycle::Sealed {
                    break;
                }
                accept_clean(
                    store.execute_current(storage.current_compare_provider_completion(build)),
                    "provider completion comparison",
                    receipts,
                );
                build = storage
                    .provider_item_build(
                        store,
                        prepared.initial_build().item_id(),
                        SyndicPointReadLimit::new(1_000_000).unwrap(),
                    )
                    .unwrap()
                    .unwrap_or_else(|| panic!("provider build disappeared during comparison"));
            }
            assert_eq!(build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
            let frame = prepared.target().clone();
            let operation = format!(
                "provider-frame publication for source event {}",
                prepared.initial_build().source_event().get(),
            );
            accept_clean(
                store.execute_current(
                    storage.current_admit_live_source_event(
                        LiveSourceEvent::new(
                            turn.thread,
                            turn.turn,
                            turn.state_revision,
                            turn.gate_revision,
                            prepared.initial_build().source_event(),
                            Some(turn.source.clone()),
                            SourceEventPayload::ItemFrame {
                                item_id: prepared.initial_build().item_id(),
                                frame: Box::new(frame),
                            },
                            turn.observed_at,
                        )
                        .unwrap(),
                    ),
                ),
                &operation,
                receipts,
            );
            turn.state_revision = turn.state_revision.checked_next().unwrap();
            converge_transcript(store, storage, turn.thread, receipts);
        }
        converge_item_projection(
            store,
            storage,
            self.prepared
                .first()
                .expect("provider fixture has no prepared frame")
                .initial_build()
                .item_id(),
            receipts,
        );
    }
}

pub(super) fn converge_item_projection(
    store: &HomeStore,
    storage: &SyndicStorage,
    item_id: SyndicItemId,
    receipts: &mut Vec<CommitReceipt>,
) {
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let item = storage
        .canonical_item(store, item_id, limit)
        .unwrap()
        .unwrap_or_else(|| panic!("provider fixture canonical item disappeared"));
    if item.projection_source().is_none() {
        return;
    }
    let head = storage.item_projection_head(store, item_id, limit).unwrap();
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
    accept_clean(
        store.execute_current(storage.current_start_item_projection_build(
            StartItemProjectionBuild::new(item_id, item.revision(), generation),
        )),
        "item-projection build start",
        receipts,
    );
    for _ in 0..4_096 {
        if storage
            .item_projection_head(store, item_id, limit)
            .unwrap()
            .as_ref()
            .is_some_and(|head| head.lifecycle() == ProjectionLifecycle::Current)
        {
            return;
        }
        let build = storage
            .item_projection_build(store, item_id, generation, limit)
            .unwrap()
            .unwrap_or_else(|| panic!("provider fixture item-projection build disappeared"));
        accept_clean(
            store.execute_current(storage.current_advance_item_projection_build(
                AdvanceItemProjectionBuild::new(item_id, generation, build.revision()),
            )),
            "item-projection build advance",
            receipts,
        );
    }
    panic!("bounded provider-fixture item projection did not finish");
}

pub(super) fn converge_transcript(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    receipts: &mut Vec<CommitReceipt>,
) {
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let thread = storage
        .thread(store, thread_id, limit)
        .unwrap()
        .unwrap_or_else(|| panic!("provider fixture thread disappeared"));
    let head = storage
        .transcript_view_head(store, thread_id, limit)
        .unwrap()
        .unwrap_or_else(|| panic!("provider fixture transcript head disappeared"));
    if head.lifecycle() == ProjectionLifecycle::Current {
        return;
    }
    let generation = head.generation();
    accept_clean(
        store.execute_current(
            storage.current_start_transcript_build(StartTranscriptBuild::new(
                thread_id,
                thread.revision(),
                head.revision(),
            )),
        ),
        "transcript-build start",
        receipts,
    );
    for _ in 0..4_096 {
        let build = storage
            .transcript_build(store, thread_id, generation, limit)
            .unwrap()
            .unwrap_or_else(|| panic!("provider fixture transcript build disappeared"));
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        accept_clean(
            store.execute_current(storage.current_advance_transcript_build(
                AdvanceTranscriptBuild::new(thread_id, generation, build.revision()),
            )),
            "transcript-build advance",
            receipts,
        );
    }
    panic!("bounded provider-fixture transcript build did not finish");
}

pub(super) fn accept_clean(
    outcome: CommandOutcome,
    operation: &str,
    receipts: &mut Vec<CommitReceipt>,
) {
    match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
            local_finalization: _,
        } => receipts.push(receipt),
        outcome => panic!("expected clean {operation}, got {outcome:?}"),
    }
}
