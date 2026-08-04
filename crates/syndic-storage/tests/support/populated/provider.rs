use std::convert::Infallible;

use super::*;

pub(super) struct ProviderItemFixture {
    pub(super) frames: Vec<SealedProviderFrameReference>,
    pub(super) canonical: SealedProviderFrameReference,
    pub(super) records: Vec<FixtureRecord>,
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
    provider_item_fixture(
        item,
        turn,
        source,
        frames,
        matches!(state, AgentItemFixtureState::Finalized),
    )
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
        false,
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
        true,
    )
}

fn provider_item_fixture(
    item: SyndicItemId,
    turn: SyndicTurnId,
    source: CasItemSource,
    frames: Vec<(SourceEventSequence, ProviderItemFrameV1)>,
    finalized: bool,
) -> ProviderItemFixture {
    let mut published = Vec::with_capacity(frames.len());
    let mut records = Vec::new();
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
        let mut build = stage_provider_frame(
            &prepared,
            prepared.initial_build().clone(),
            &mut |batch: &ProviderFrameStageBatch| {
                records.extend(
                    batch
                        .chunks()
                        .iter()
                        .cloned()
                        .map(FixtureRecord::ContentChunk),
                );
                records.extend(
                    batch
                        .byte_spans()
                        .iter()
                        .copied()
                        .map(FixtureRecord::ContentByteSpan),
                );
                records.extend(
                    batch
                        .narrative_spans()
                        .iter()
                        .copied()
                        .map(FixtureRecord::ProviderNarrativeSpan),
                );
                Ok::<_, Infallible>(())
            },
        )
        .unwrap();
        if build
            .completion_check()
            .is_some_and(|check| !check.state().is_terminal())
        {
            build = build
                .advance_completion(ProviderNarrativeCompletionState::Equal)
                .unwrap();
        }
        assert_eq!(build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
        let target = prepared.target().clone();
        prior = Some(target.clone());
        published.push(target);
    }
    let latest = published.last().cloned().unwrap();
    assert!(!finalized || latest.stream_state().is_complete());
    let (canonical, manifest) = fixture_provider_content_manifest(item, &latest, finalized);
    records.push(FixtureRecord::ContentManifest(manifest));
    ProviderItemFixture {
        frames: published,
        canonical,
        records,
    }
}
