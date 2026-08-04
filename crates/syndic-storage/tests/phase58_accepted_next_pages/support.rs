use beryl_model::{
    AcceptedInputRevision, BindingRevision, DraftRevision, InputGateRevision, SyndicDraftId,
    SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use crate::support::{
    draft_id,
    populated::{active_snapshot, active_turn, cas_thread, cas_turn},
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

fn source_draft(seed: u8, ordinal: u64) -> SyndicDraftId {
    let mut bytes = [0_u8; 16];
    bytes[0] = seed;
    bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
    SyndicDraftId::from_bytes(bytes)
}

pub fn next_turn_records(
    seed: u8,
    thread: SyndicThreadId,
    specs: &[GenerationSpec],
) -> Vec<FixtureRecord> {
    assert!(!specs.is_empty());
    assert!(specs.iter().all(|spec| spec.candidate_count != 0));
    let total = specs.iter().map(|spec| spec.row_count()).sum::<u64>();
    let live_next = specs.iter().map(|spec| spec.candidate_count).sum::<u64>();
    let current_draft = draft_id(seed.wrapping_add(1));
    let prior_turn = SyndicTurnId::from_bytes([seed; 16]);
    let prior_digest = root_turn_chain_digest(prior_turn);
    let mut records = thread_records(thread, current_draft, Some(prior_turn), prior_digest);
    let content = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::Draft(draft) if draft.id() == current_draft => Some(draft.content()),
            _ => None,
        })
        .expect("empty thread fixture contains its current draft");
    let final_thread_revision = ThreadRevision::new(total + 1).unwrap();
    let final_gate_revision = InputGateRevision::new(total + 1).unwrap();
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
                    current.image_label_frontiers(),
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
                    summary.last_activity_at(),
                );
            }
            _ => {}
        }
    }

    let all_source_drafts = (1..=total)
        .map(|ordinal| source_draft(seed, ordinal))
        .collect::<Vec<_>>();
    let mut ordinal_value = 1_u64;
    for (generation_index, spec) in specs.iter().copied().enumerate() {
        let generation =
            AcceptedRouteGeneration::new(u64::try_from(generation_index).unwrap() + 1).unwrap();
        let first = AcceptedInputOrdinal::new(ordinal_value).unwrap();
        let last_value = ordinal_value + spec.row_count() - 1;
        let last = AcceptedInputOrdinal::new(last_value).unwrap();
        records.push(FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                generation,
                AcceptedRouteRevision::FIRST,
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
            AcceptedNextSourceRecord::new(
                thread,
                generation,
                AcceptedRouteRevision::FIRST,
                first,
                last,
            ),
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
                FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
                    input_id,
                    thread,
                    generation,
                    ordinal,
                    AcceptedInputRevision::new(1).unwrap(),
                    if local_index < spec.terminal_prefix {
                        AcceptedRouteLeafState::Routed
                    } else {
                        AcceptedRouteLeafState::NextTurn(spec.reason)
                    },
                    if local_index < spec.terminal_prefix {
                        AcceptedInputLifecycle::Delivered
                    } else {
                        AcceptedInputLifecycle::Admitted
                    },
                )),
            ]);
            ordinal_value += 1;
        }
    }
    records
}

pub fn set_gate_state(
    records: &mut [FixtureRecord],
    thread: SyndicThreadId,
    state: InputGateState,
) {
    for record in records {
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

pub fn set_projection_lost(
    records: &mut [FixtureRecord],
    thread: SyndicThreadId,
    generation_id: AcceptedRouteGeneration,
) {
    let pending = PendingSteeringTargetProof::new(
        BindingRevision::new(1).unwrap(),
        active_snapshot(),
        active_turn(),
        cas_thread(),
    );
    let lost = AcceptedRouteProjectionLostProof::new(
        AcceptedRouteLostTarget::Steering(SteeringTargetProof::new(pending, cas_turn())),
        AcceptedRouteAbandonmentProof::new(
            BindingRevision::new(1).unwrap(),
            InputGateRevision::new(1).unwrap(),
            AcceptedRouteHeadProof::new(generation_id, AcceptedRouteRevision::FIRST),
            AcceptedRouteAbandonmentKind::Generic,
        ),
        BindingRevision::new(2).unwrap(),
        active_snapshot(),
        cas_thread(),
    );
    for record in records {
        match record {
            FixtureRecord::AcceptedRouteGeneration(generation)
                if generation.thread_id() == thread && generation.generation() == generation_id =>
            {
                *generation = AcceptedRouteGenerationRecord::new(
                    thread,
                    generation_id,
                    generation.revision(),
                    AcceptedRouteTarget::ProjectionLost(lost.clone()),
                    generation.first_ordinal(),
                    generation.last_ordinal(),
                    generation.input_count(),
                    generation.ready_retryable_count(),
                    generation.delivering_count(),
                    generation.next_turn_count(),
                    generation.terminal_count(),
                    generation.live_logical_utf8_bytes(),
                    generation.delivering_logical_utf8_bytes(),
                )
                .unwrap();
            }
            FixtureRecord::AcceptedRouteLeaf(leaf)
                if leaf.thread_id() == thread
                    && leaf.generation() == generation_id
                    && !leaf.lifecycle().is_terminal() =>
            {
                *leaf = AcceptedRouteLeafRecord::new(
                    leaf.input_id(),
                    leaf.thread_id(),
                    leaf.generation(),
                    leaf.ordinal(),
                    leaf.revision(),
                    AcceptedRouteLeafState::Routed,
                    leaf.lifecycle(),
                );
            }
            _ => {}
        }
    }
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
