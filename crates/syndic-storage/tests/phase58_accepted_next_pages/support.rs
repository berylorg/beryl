use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, SyndicDraftId, SyndicThreadId,
    SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{FixtureRecord, fixture_route_leaf_with_transition};
use syndic_storage::*;

use crate::support::{
    composer_content_records, fixture_turn_state, item_free_transcript_build_records,
    thread_records, timestamp,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationSpec {
    pub terminal_prefix: u64,
    pub candidate_count: u64,
    pub reason: NextTurnReason,
}

impl GenerationSpec {
    pub const fn new(terminal_prefix: u64, candidate_count: u64, reason: NextTurnReason) -> Self {
        Self {
            terminal_prefix,
            candidate_count,
            reason,
        }
    }

    fn row_count(self) -> u64 {
        self.terminal_prefix + self.candidate_count
    }
}

fn source_draft(seed: u64, ordinal: u64) -> SyndicDraftId {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
    SyndicDraftId::from_bytes(bytes)
}

fn source_turn(seed: u64) -> SyndicTurnId {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    bytes[8] = 0xff;
    bytes[15] = 0xff;
    SyndicTurnId::from_bytes(bytes)
}

pub fn next_turn_records(
    seed: u64,
    thread: SyndicThreadId,
    specs: &[GenerationSpec],
) -> Vec<FixtureRecord> {
    assert!(!specs.is_empty());
    assert!(specs.iter().all(|spec| spec.candidate_count != 0));
    let total = specs.iter().map(|spec| spec.row_count()).sum::<u64>();
    let live_next = specs.iter().map(|spec| spec.candidate_count).sum::<u64>();
    let terminal_count = specs.iter().map(|spec| spec.terminal_prefix).sum::<u64>();
    let current_draft = source_draft(seed, 0);
    let prior_turn = source_turn(seed);
    let prior_digest = root_turn_chain_digest(prior_turn);
    let mut records = thread_records(thread, current_draft, Some(prior_turn), prior_digest);
    let content = composer_content_records(&ComposerPayload::default()).0;
    let final_thread_revision = ThreadRevision::new(total + 1).unwrap();
    let final_gate_revision = InputGateRevision::new(total + 1 + (terminal_count * 2)).unwrap();
    let generation_high_water =
        AcceptedRouteGeneration::new(u64::try_from(specs.len()).unwrap()).unwrap();

    for record in &mut records {
        match record {
            FixtureRecord::Thread(current) if current.id() == thread => {
                *current = ThreadRecord::new(
                    thread,
                    SelectedPathProof::new(
                        current.committed_tail(),
                        final_thread_revision,
                        current.selected_path_digest(),
                    ),
                    current.current_draft_id(),
                    current.lineage(),
                    current.context_owner_id(),
                );
            }
            FixtureRecord::DraftByThread(reverse) if reverse.thread_id() == thread => {
                *reverse = DraftByThreadRecord::new(
                    thread,
                    reverse.draft_id(),
                    reverse.draft_revision(),
                    final_thread_revision,
                );
            }
            FixtureRecord::InputGate(gate) if gate.thread_id() == thread => {
                *gate = InputGateRecord::new(
                    thread,
                    final_gate_revision,
                    InputGateState::Idle,
                    total,
                    Some(generation_high_water),
                    None,
                    0,
                    live_next,
                    0,
                )
                .unwrap();
            }
            FixtureRecord::HistorySummary(summary) if summary.thread_id() == thread => {
                *summary = HistorySummaryRecord::new(
                    thread,
                    summary.revision().checked_next().unwrap(),
                    final_thread_revision,
                    summary.committed_tail(),
                    summary.selected_path_digest(),
                    summary.complete(),
                    timestamp(total.max(5)),
                );
            }
            FixtureRecord::TranscriptViewHead(view) if view.thread_id() == thread => {
                *view = TranscriptViewHeadRecord::new(
                    thread,
                    view.generation(),
                    view.revision(),
                    0,
                    Some(prior_turn),
                    prior_digest,
                    ProjectionLifecycle::Current,
                );
            }
            FixtureRecord::Binding(binding) if binding.thread_id() == thread => {
                *binding = BindingRecord::new(
                    thread,
                    binding.revision(),
                    SelectedPathProof::new(
                        Some(prior_turn),
                        ThreadRevision::new(1).unwrap(),
                        prior_digest,
                    ),
                    binding.state().clone(),
                );
            }
            FixtureRecord::BindingHead(head) if head.thread_id() == thread => {
                *head =
                    BindingHeadRecord::new(thread, head.revision(), head.lifecycle(), prior_digest);
            }
            _ => {}
        }
    }

    let all_source_drafts = (1..=total)
        .map(|ordinal| source_draft(seed, ordinal))
        .collect::<Vec<_>>();
    let mut ordinal_value = 1_u64;
    let mut completed_before_generation = 0_u64;
    for (generation_index, spec) in specs.iter().copied().enumerate() {
        let generation =
            AcceptedRouteGeneration::new(u64::try_from(generation_index).unwrap() + 1).unwrap();
        let first = AcceptedInputOrdinal::new(ordinal_value).unwrap();
        let last_value = ordinal_value + spec.row_count() - 1;
        let last = AcceptedInputOrdinal::new(last_value).unwrap();
        let generation_revision =
            AcceptedRouteRevision::new(1 + (spec.terminal_prefix * 2)).unwrap();
        records.push(FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                generation,
                generation_revision,
                AcceptedRouteTarget::NextTurn(spec.reason),
                Some(first),
                Some(last),
                spec.row_count(),
                0,
                0,
                spec.candidate_count,
                spec.terminal_prefix,
                0,
                0,
            )
            .unwrap(),
        ));
        records.push(FixtureRecord::AcceptedNextSource(
            AcceptedNextSourceRecord::new(thread, generation, generation_revision, first, last),
        ));
        for local_index in 0..spec.row_count() {
            let ordinal = AcceptedInputOrdinal::new(ordinal_value).unwrap();
            let draft = all_source_drafts[usize::try_from(ordinal_value - 1).unwrap()];
            let replacement = if ordinal_value < total {
                all_source_drafts[usize::try_from(ordinal_value).unwrap()]
            } else {
                current_draft
            };
            let input_id = draft.accepted_input_id();
            records.extend([
                FixtureRecord::AcceptedInput(
                    AcceptedInputRecord::new(
                        input_id,
                        thread,
                        ordinal,
                        AcceptedInputAdmissionProof::new(
                            ThreadRevision::new(ordinal_value).unwrap(),
                            draft,
                            DraftRevision::new(1).unwrap(),
                            InputGateRevision::new(ordinal_value).unwrap(),
                            replacement,
                        )
                        .unwrap(),
                        generation,
                        content,
                        None,
                        timestamp(ordinal_value),
                    )
                    .unwrap(),
                ),
                FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                    thread, ordinal, input_id, generation,
                )),
                FixtureRecord::AcceptedRouteLeaf(if local_index < spec.terminal_prefix {
                    fixture_route_leaf_with_transition(
                        AcceptedRouteLeafRecord::new(
                            input_id,
                            thread,
                            generation,
                            ordinal,
                            AcceptedInputRevision::new(3).unwrap(),
                            AcceptedRouteLeafState::Routed,
                            AcceptedInputLifecycle::Delivered,
                        ),
                        AcceptedRouteLeafTransitionProof::new(
                            InputGateRevision::new(
                                total + 2 + ((completed_before_generation + local_index) * 2),
                            )
                            .unwrap(),
                            AcceptedRouteHeadProof::new(
                                generation,
                                AcceptedRouteRevision::new(2 + (local_index * 2)).unwrap(),
                            ),
                            AcceptedInputRevision::new(2).unwrap(),
                            AcceptedRouteLeafTransitionKind::Complete,
                        ),
                    )
                } else {
                    AcceptedRouteLeafRecord::new(
                        input_id,
                        thread,
                        generation,
                        ordinal,
                        AcceptedInputRevision::new(1).unwrap(),
                        AcceptedRouteLeafState::NextTurn(spec.reason),
                        AcceptedInputLifecycle::Admitted,
                    )
                }),
            ]);
            ordinal_value += 1;
        }
        completed_before_generation += spec.terminal_prefix;
    }
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            prior_turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            prior_digest,
            timestamp(1),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            prior_turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Failed,
            1,
            0,
            timestamp(5),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                prior_turn,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
    ]);
    records.extend(item_free_transcript_build_records(
        thread,
        final_thread_revision,
        &[(
            prior_turn,
            prior_digest,
            TurnLifecycle::Failed,
            1,
            timestamp(5),
        )],
    ));
    records
}

pub fn set_gate_state(
    records: &mut [FixtureRecord],
    thread: SyndicThreadId,
    state: InputGateState,
) {
    for record in records.iter_mut() {
        if let FixtureRecord::InputGate(gate) = record
            && gate.thread_id() == thread
        {
            *gate = InputGateRecord::new(
                thread,
                gate.revision(),
                state,
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                gate.selected_route(),
                gate.live_steering_count(),
                gate.live_next_turn_count(),
                gate.live_logical_utf8_bytes(),
            )
            .unwrap();
            return;
        }
    }
    panic!("fixture gate is missing");
}

pub fn corrupt_effective_leaves_as_terminal(
    records: &mut [FixtureRecord],
    thread: SyndicThreadId,
    generation_id: AcceptedRouteGeneration,
) {
    for record in records {
        if let FixtureRecord::AcceptedRouteLeaf(leaf) = record
            && leaf.thread_id() == thread
            && leaf.generation() == generation_id
            && !leaf.lifecycle().is_terminal()
        {
            *leaf = AcceptedRouteLeafRecord::new(
                leaf.input_id(),
                leaf.thread_id(),
                leaf.generation(),
                leaf.ordinal(),
                leaf.revision(),
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Delivered,
            );
        }
    }
}
