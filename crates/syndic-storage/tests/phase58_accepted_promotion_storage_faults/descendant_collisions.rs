use beryl_model::SyndicPathDigest;
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};

use super::*;

#[derive(Clone, Copy, Debug)]
enum DescendantDrift {
    MissingDraft,
    MissingSuccessorTurn,
    DraftReverse,
    SummaryActivity,
    GateAggregate,
    ActivityHead,
    ActivitySource,
    TranscriptLifecycle,
    TranscriptGeneration,
    ThreadPath,
    BindingIdentity,
}

impl DescendantDrift {
    const ALL: [Self; 11] = [
        Self::MissingDraft,
        Self::MissingSuccessorTurn,
        Self::DraftReverse,
        Self::SummaryActivity,
        Self::GateAggregate,
        Self::ActivityHead,
        Self::ActivitySource,
        Self::TranscriptLifecycle,
        Self::TranscriptGeneration,
        Self::ThreadPath,
        Self::BindingIdentity,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::MissingDraft => "missing-draft",
            Self::MissingSuccessorTurn => "missing-successor-turn",
            Self::DraftReverse => "draft-reverse",
            Self::SummaryActivity => "summary-activity",
            Self::GateAggregate => "gate-aggregate",
            Self::ActivityHead => "activity-head",
            Self::ActivitySource => "activity-source",
            Self::TranscriptLifecycle => "transcript-lifecycle",
            Self::TranscriptGeneration => "transcript-generation",
            Self::ThreadPath => "thread-path",
            Self::BindingIdentity => "binding-identity",
        }
    }

    fn fixture_batch(
        self,
        store: &HomeStore,
        storage: SyndicStorage,
        fixture: &Fixture,
        request: &PromoteAcceptedInput,
    ) -> FixtureBatch {
        let mut batch = FixtureBatch::new();
        match self {
            Self::MissingDraft => {
                batch
                    .delete(FixtureDelete::Draft(fixture.current_draft))
                    .unwrap();
            }
            Self::MissingSuccessorTurn => {
                batch
                    .delete(FixtureDelete::Turn(request.successor_turn_id()))
                    .unwrap();
            }
            Self::DraftReverse => {
                let current = storage
                    .current_draft(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                batch
                    .put(FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                        fixture.thread,
                        current.draft().id(),
                        current.draft().revision().checked_next().unwrap(),
                        current.thread().revision(),
                    )))
                    .unwrap();
            }
            Self::SummaryActivity => {
                let summary = storage
                    .history_summary(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                batch
                    .put(FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                        summary.thread_id(),
                        summary.revision().checked_next().unwrap(),
                        summary.thread_revision(),
                        summary.committed_tail(),
                        summary.selected_path_digest(),
                        summary.complete(),
                        timestamp(99),
                    )))
                    .unwrap();
            }
            Self::GateAggregate => {
                let gate = storage
                    .input_gate(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                batch
                    .put(FixtureRecord::InputGate(
                        InputGateRecord::new(
                            gate.thread_id(),
                            gate.revision(),
                            gate.state().clone(),
                            gate.accepted_high_water(),
                            gate.route_generation_high_water(),
                            gate.selected_route(),
                            gate.live_steering_count(),
                            gate.live_next_turn_count(),
                            gate.live_logical_utf8_bytes().checked_add(1).unwrap(),
                        )
                        .unwrap(),
                    ))
                    .unwrap();
            }
            _ => {
                self.put_projection_or_identity_drift(&mut batch, store, storage, fixture, request)
            }
        }
        batch
    }
}

impl DescendantDrift {
    fn put_projection_or_identity_drift(
        self,
        batch: &mut FixtureBatch,
        store: &HomeStore,
        storage: SyndicStorage,
        fixture: &Fixture,
        request: &PromoteAcceptedInput,
    ) {
        let record = match self {
            Self::ActivityHead => {
                let head = storage
                    .activity_query_head(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                FixtureRecord::ActivityQueryHead(
                    ActivityQueryHeadRecord::new(
                        head.thread_id(),
                        head.work_period(),
                        head.source(),
                        head.source_active(),
                        head.source_frontier(),
                        head.revision().checked_next().unwrap(),
                        head.source_count(),
                        head.logical_row_count(),
                        head.running_row_count(),
                        head.completed_row_count(),
                        head.completed_stored_bytes(),
                        head.completed_retention_cutoff(),
                        head.lifecycle(),
                    )
                    .unwrap(),
                )
            }
            Self::ActivitySource => {
                let head = storage
                    .activity_query_head(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                let source = head.source().expect("promoted turn is the root source");
                assert_eq!(source.turn_id(), request.successor_turn_id());
                FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
                    fixture.thread,
                    head.work_period(),
                    source,
                    None,
                    0,
                    false,
                    None,
                ))
            }
            Self::TranscriptLifecycle => {
                let head = storage
                    .transcript_view_head(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
                    head.thread_id(),
                    head.generation(),
                    head.revision(),
                    head.entry_count(),
                    head.committed_tail(),
                    head.selected_path_digest(),
                    ProjectionLifecycle::Current,
                ))
            }
            Self::TranscriptGeneration => {
                let head = storage
                    .transcript_view_head(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
                    head.thread_id(),
                    head.generation().checked_next().unwrap(),
                    head.revision(),
                    head.entry_count(),
                    head.committed_tail(),
                    head.selected_path_digest(),
                    head.lifecycle(),
                ))
            }
            Self::ThreadPath => {
                let thread = storage
                    .thread(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                FixtureRecord::Thread(ThreadRecord::new(
                    thread.id(),
                    SelectedPathProof::new(
                        thread.committed_tail(),
                        thread.revision(),
                        SyndicPathDigest::from_bytes([0x6d; 32]),
                    ),
                    thread.current_draft_id(),
                    thread.lineage(),
                    thread.image_label_frontiers(),
                    thread.context_owner_id(),
                ))
            }
            Self::BindingIdentity => {
                let current = storage
                    .current_binding(store, fixture.thread, limit())
                    .unwrap()
                    .unwrap();
                let binding = current.binding();
                FixtureRecord::Binding(BindingRecord::new(
                    binding.thread_id(),
                    binding.revision(),
                    binding.selected_path(),
                    BindingState::unbound("incompatible promotion descendant").unwrap(),
                ))
            }
            Self::MissingDraft
            | Self::MissingSuccessorTurn
            | Self::DraftReverse
            | Self::SummaryActivity
            | Self::GateAggregate => unreachable!(),
        };
        batch.put(record).unwrap();
    }
}

#[test]
fn similar_but_incoherent_promotion_descendants_are_collisions() {
    for (offset, drift) in DescendantDrift::ALL.into_iter().enumerate() {
        let fixture_seed = 100_u8.checked_add(offset as u8).unwrap();
        let fixture = promotion_fixture(fixture_seed, id(fixture_seed));
        let name = format!("phase61-promotion-descendant-{}", drift.name());
        let (_home, store, storage) = seed(&name, fixture.records.clone());
        let request = promotion(
            &store,
            storage,
            SyndicTurnId::from_bytes([200_u8.checked_add(offset as u8).unwrap(); 16]),
            SyndicItemId::from_bytes([220_u8.checked_add(offset as u8).unwrap(); 16]),
        );
        match execute_promotion(&store, storage, request.clone()) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => {
                panic!(
                    "expected descendant promotion to commit without later failure, got {outcome:?}"
                )
            }
        }
        assert_eq!(
            storage
                .accepted_input_promotion_status(&store, &request, limit())
                .unwrap(),
            AcceptedInputPromotionStatus::Exact,
        );

        commit(
            &store,
            storage,
            drift.fixture_batch(&store, storage, &fixture, &request),
        );
        assert_eq!(
            storage
                .accepted_input_promotion_status(&store, &request, limit())
                .unwrap(),
            AcceptedInputPromotionStatus::Collision,
            "{} drift resembled a compatible descendant",
            drift.name(),
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum PriorSourceDrift {
    MissingParentTurn,
    MissingParentState,
    NonTerminalParentState,
}

impl PriorSourceDrift {
    const ALL: [Self; 3] = [
        Self::MissingParentTurn,
        Self::MissingParentState,
        Self::NonTerminalParentState,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::MissingParentTurn => "missing-parent-turn",
            Self::MissingParentState => "missing-parent-state",
            Self::NonTerminalParentState => "nonterminal-parent-state",
        }
    }

    fn fixture_batch(self, parent: SyndicTurnId) -> FixtureBatch {
        let mut batch = FixtureBatch::new();
        match self {
            Self::MissingParentTurn => {
                batch.delete(FixtureDelete::Turn(parent)).unwrap();
            }
            Self::MissingParentState => {
                batch.delete(FixtureDelete::TurnState(parent)).unwrap();
            }
            Self::NonTerminalParentState => {
                batch
                    .put(FixtureRecord::TurnState(fixture_turn_state(
                        parent,
                        TurnStateRevision::FIRST,
                        TurnLifecycle::Pending,
                        0,
                        0,
                        timestamp(5),
                    )))
                    .unwrap();
            }
        }
        batch
    }
}

#[test]
fn prior_requires_the_complete_terminal_source_parent() {
    for (offset, drift) in PriorSourceDrift::ALL.into_iter().enumerate() {
        let fixture_seed = 140_u8.checked_add(offset as u8).unwrap();
        let fixture = promotion_fixture(fixture_seed, id(fixture_seed));
        let parent = fixture
            .records
            .iter()
            .find_map(|record| match record {
                FixtureRecord::Thread(thread) if thread.id() == fixture.thread => {
                    thread.committed_tail()
                }
                _ => None,
            })
            .unwrap();
        let name = format!("phase61-promotion-prior-source-{}", drift.name());
        let (_home, store, storage) = seed(&name, fixture.records.clone());
        let request = promotion(
            &store,
            storage,
            SyndicTurnId::from_bytes([230_u8.checked_add(offset as u8).unwrap(); 16]),
            SyndicItemId::from_bytes([240_u8.checked_add(offset as u8).unwrap(); 16]),
        );
        assert_eq!(
            storage
                .accepted_input_promotion_status(&store, &request, limit())
                .unwrap(),
            AcceptedInputPromotionStatus::Prior,
        );

        commit(&store, storage, drift.fixture_batch(parent));
        assert_eq!(
            storage
                .accepted_input_promotion_status(&store, &request, limit())
                .unwrap(),
            AcceptedInputPromotionStatus::Collision,
            "{} drift retained an incomplete Prior classification",
            drift.name(),
        );
    }
}
