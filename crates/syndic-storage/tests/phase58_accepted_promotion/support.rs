use beryl_model::{
    DraftRevision, SyndicAcceptedInputId, SyndicDraftId, SyndicThreadId, SyndicTurnId,
    ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use crate::{
    accepted_next_support::{GenerationSpec, next_turn_records},
    support::{
        composer_content_records, fixture_turn_state, item_free_transcript_build_records, timestamp,
    },
};

#[derive(Clone)]
pub struct Fixture {
    pub records: Vec<FixtureRecord>,
    pub thread: SyndicThreadId,
    pub current_draft: SyndicDraftId,
    pub accepted_input: SyndicAcceptedInputId,
    pub accepted_content: ContentReference,
}

pub struct NewerGenerationFixture {
    pub fixture: Fixture,
    pub newer_generation: AcceptedRouteGeneration,
    pub newer_head: AcceptedRouteGenerationHeadRecord,
    pub newer_accepted_input: SyndicAcceptedInputId,
}

pub fn promotion_fixture(seed: u8, thread: SyndicThreadId) -> Fixture {
    promotion_fixture_for_generations(
        seed,
        thread,
        &[GenerationSpec::new(0, 1, NextTurnReason::PendingTurn)],
    )
}

pub fn promotion_fixture_with_newer_generation(
    seed: u8,
    thread: SyndicThreadId,
) -> NewerGenerationFixture {
    let newer_generation = AcceptedRouteGeneration::new(2).unwrap();
    let mut fixture = promotion_fixture_for_generations(
        seed,
        thread,
        &[
            GenerationSpec::new(0, 1, NextTurnReason::PendingTurn),
            GenerationSpec::new(0, 1, NextTurnReason::PendingTurn),
        ],
    );
    let newer_accepted_input = fixture
        .records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(record)
                if record.thread_id() == thread
                    && record.route_generation() == newer_generation =>
            {
                Some(record.id())
            }
            _ => None,
        })
        .expect("second promotion generation owns accepted input");
    let newer_head = AcceptedRouteGenerationHeadRecord::new(
        thread,
        AcceptedRouteHeadProof::new(newer_generation, AcceptedRouteRevision::FIRST),
    );
    fixture
        .records
        .push(FixtureRecord::AcceptedRouteGenerationHead(newer_head));
    NewerGenerationFixture {
        fixture,
        newer_generation,
        newer_head,
        newer_accepted_input,
    }
}

fn promotion_fixture_for_generations(
    seed: u8,
    thread: SyndicThreadId,
    generations: &[GenerationSpec],
) -> Fixture {
    let mut records = next_turn_records(seed, thread, generations);
    let parent = SyndicTurnId::from_bytes([seed.wrapping_add(10); 16]);
    let parent_digest = root_turn_chain_digest(parent);
    let current_draft = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::Thread(record) if record.id() == thread => {
                Some(record.current_draft_id())
            }
            _ => None,
        })
        .expect("next fixture owns a thread");
    let thread_revision = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::Thread(record) if record.id() == thread => Some(record.revision()),
            _ => None,
        })
        .expect("next fixture owns a thread revision");
    let accepted_input = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(record)
                if record.thread_id() == thread
                    && record.route_generation() == AcceptedRouteGeneration::FIRST =>
            {
                Some(record.id())
            }
            _ => None,
        })
        .expect("first promotion generation owns accepted input");
    let current_draft_created_at = records
        .iter()
        .filter_map(|record| match record {
            FixtureRecord::AcceptedInput(record) if record.thread_id() == thread => {
                Some((record.ordinal(), record.admitted_at()))
            }
            _ => None,
        })
        .max_by_key(|(ordinal, _)| *ordinal)
        .map(|(_, admitted_at)| admitted_at)
        .expect("promotion fixture owns an accepted-input frontier");
    let accepted_payload =
        ComposerPayload::new(vec![ComposerAtom::text("queued input").unwrap()]).unwrap();
    let draft_payload =
        ComposerPayload::new(vec![ComposerAtom::text("unsent draft").unwrap()]).unwrap();
    let (accepted_content, accepted_content_records) = composer_content_records(&accepted_payload);
    let (draft_content, draft_content_records) = composer_content_records(&draft_payload);
    records.retain(|record| !matches!(record, FixtureRecord::TranscriptBuild(_)));
    rewrite_base_records(
        &mut records,
        thread,
        current_draft,
        parent,
        parent_digest,
        accepted_content,
        draft_content,
        thread_revision,
        current_draft_created_at,
    );
    records.extend(accepted_content_records);
    records.extend(draft_content_records);
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            parent,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            parent_digest,
            timestamp(1),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            parent,
            TurnStateRevision::FIRST,
            TurnLifecycle::Failed,
            1,
            0,
            timestamp(5),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                parent,
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
        thread_revision,
        &[(
            parent,
            parent_digest,
            TurnLifecycle::Failed,
            1,
            timestamp(5),
        )],
    ));
    Fixture {
        records,
        thread,
        current_draft,
        accepted_input,
        accepted_content,
    }
}

#[allow(clippy::too_many_arguments)]
fn rewrite_base_records(
    records: &mut [FixtureRecord],
    thread: SyndicThreadId,
    current_draft: SyndicDraftId,
    parent: SyndicTurnId,
    parent_digest: beryl_model::SyndicPathDigest,
    accepted_content: ContentReference,
    draft_content: ContentReference,
    thread_revision: ThreadRevision,
    current_draft_created_at: SyndicTimestamp,
) {
    let draft_revision = DraftRevision::new(2).unwrap();
    let accepted_bytes = accepted_content.summary().logical_utf8_bytes();
    for record in records {
        match record {
            FixtureRecord::Thread(current) if current.id() == thread => {
                *current = ThreadRecord::new(
                    thread,
                    SelectedPathProof::new(Some(parent), thread_revision, parent_digest),
                    current.current_draft_id(),
                    current.lineage(),
                    current.image_label_frontiers(),
                    current.context_owner_id(),
                );
            }
            FixtureRecord::Draft(current) if current.id() == current_draft => {
                *current = DraftRecord::new(
                    current.id(),
                    thread,
                    draft_revision,
                    current.submission_intent(),
                    draft_content,
                    current_draft_created_at,
                    timestamp(15),
                );
            }
            FixtureRecord::DraftByThread(current) if current.thread_id() == thread => {
                *current = DraftByThreadRecord::new(
                    thread,
                    current_draft,
                    draft_revision,
                    thread_revision,
                );
            }
            FixtureRecord::InputGate(current) if current.thread_id() == thread => {
                let live_count = current
                    .live_steering_count()
                    .checked_add(current.live_next_turn_count())
                    .expect("promotion fixture live input count fits u64");
                *current = InputGateRecord::new(
                    thread,
                    current.revision(),
                    InputGateState::Idle,
                    current.accepted_high_water(),
                    current.route_generation_high_water(),
                    current.selected_route(),
                    current.live_steering_count(),
                    current.live_next_turn_count(),
                    accepted_bytes
                        .checked_mul(live_count)
                        .expect("promotion fixture live input bytes fit u64"),
                )
                .unwrap();
            }
            FixtureRecord::TranscriptViewHead(current) if current.thread_id() == thread => {
                *current = TranscriptViewHeadRecord::new(
                    thread,
                    current.generation(),
                    current.revision(),
                    0,
                    Some(parent),
                    parent_digest,
                    ProjectionLifecycle::Current,
                );
            }
            FixtureRecord::HistorySummary(current) if current.thread_id() == thread => {
                *current = HistorySummaryRecord::new(
                    thread,
                    current.revision().checked_next().unwrap(),
                    thread_revision,
                    Some(parent),
                    parent_digest,
                    true,
                    timestamp(15),
                );
            }
            FixtureRecord::Binding(current) if current.thread_id() == thread => {
                *current = BindingRecord::new(
                    thread,
                    current.revision(),
                    SelectedPathProof::new(
                        Some(parent),
                        ThreadRevision::new(1).unwrap(),
                        parent_digest,
                    ),
                    current.state().clone(),
                );
            }
            FixtureRecord::BindingHead(current) if current.thread_id() == thread => {
                *current = BindingHeadRecord::new(
                    thread,
                    current.revision(),
                    current.lifecycle(),
                    parent_digest,
                );
            }
            FixtureRecord::AcceptedInput(current) if current.thread_id() == thread => {
                *current = AcceptedInputRecord::new(
                    current.id(),
                    thread,
                    current.ordinal(),
                    current.admission(),
                    current.route_generation(),
                    accepted_content,
                    current.asset_reference_set(),
                    current.admitted_at(),
                )
                .unwrap();
            }
            FixtureRecord::AcceptedRouteGeneration(current) if current.thread_id() == thread => {
                let live_count = current
                    .ready_retryable_count()
                    .checked_add(current.delivering_count())
                    .and_then(|count| count.checked_add(current.next_turn_count()))
                    .expect("promotion generation live input count fits u64");
                *current = AcceptedRouteGenerationRecord::new(
                    thread,
                    current.generation(),
                    current.revision(),
                    current.target().clone(),
                    current.first_ordinal(),
                    current.last_ordinal(),
                    current.input_count(),
                    current.ready_retryable_count(),
                    current.delivering_count(),
                    current.next_turn_count(),
                    current.terminal_count(),
                    accepted_bytes
                        .checked_mul(live_count)
                        .expect("promotion generation live input bytes fit u64"),
                    current.delivering_logical_utf8_bytes(),
                )
                .unwrap();
            }
            _ => {}
        }
    }
}
