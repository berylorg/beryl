#![cfg(feature = "test-faults")]

mod support;

#[path = "phase58_accepted_next_pages/support.rs"]
mod accepted_next_support;
#[path = "phase58_accepted_promotion_storage_faults/corruption.rs"]
mod corruption;
#[path = "phase58_accepted_promotion_storage_faults/deep_tail.rs"]
mod deep_tail;
#[path = "phase58_accepted_promotion_storage_faults/descendant_collisions.rs"]
mod descendant_collisions;
#[path = "phase58_accepted_promotion_storage_faults/identity.rs"]
mod identity;
#[path = "phase58_accepted_promotion/support.rs"]
mod promotion_support;

use beryl_home_store::{
    CommandError, CommandOutcome, CursorReadLimits, HomeCommand, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use promotion_support::{Fixture, promotion_fixture, seed_detached_draft_backing};
use support::{
    TestHome, batch, commit, fixture_turn_state, id, item_free_transcript_build_records, open,
    timestamp,
};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn page_limits() -> CursorReadLimits {
    CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap()
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn seed(
    name: &str,
    records: impl IntoIterator<Item = FixtureRecord>,
) -> (TestHome, HomeStore, SyndicStorage) {
    let mut records = records.into_iter().collect::<Vec<_>>();
    let current_draft = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::Thread(thread) => Some(thread.current_draft_id()),
            _ => None,
        })
        .expect("promotion fixture owns its current draft");
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let root_history = seed_detached_draft_backing(
        &store,
        storage,
        SyndicThreadId::from_bytes([0xf2; 16]),
        current_draft,
    );
    install_current_draft_root_history(&mut records, current_draft, root_history);
    commit(&store, storage, batch(records));
    (home, store, storage)
}

fn install_current_draft_root_history(
    records: &mut [FixtureRecord],
    draft_id: beryl_model::SyndicDraftId,
    root_history: syndic_storage::DraftRootHistoryPairV1,
) {
    for record in records {
        let FixtureRecord::Draft(draft) = record else {
            continue;
        };
        if draft.id() != draft_id {
            continue;
        }
        *draft = DraftRecord::new(
            draft.id(),
            draft.thread_id(),
            draft.revision(),
            draft.submission_intent(),
            root_history.clone(),
            draft.created_at(),
            draft.updated_at(),
        );
    }
}

fn candidate(store: &HomeStore, storage: SyndicStorage) -> AcceptedNextCandidate {
    let revision = storage.revision(store).unwrap();
    let sources = storage
        .accepted_next_source_page(store, revision, None, page_limits())
        .unwrap();
    assert_eq!(sources.records().len(), 1);
    storage
        .accepted_next_candidate_page(store, sources.records()[0], None, page_limits())
        .unwrap()
        .into_candidate()
        .expect("promotion fixture owns one effective next-turn input")
}

fn promotion(
    store: &HomeStore,
    storage: SyndicStorage,
    turn: SyndicTurnId,
    item: SyndicItemId,
) -> PromoteAcceptedInput {
    PromoteAcceptedInput::new(candidate(store, storage), turn, item, timestamp(20))
}

fn execute_promotion(
    store: &HomeStore,
    storage: SyndicStorage,
    promotion: PromoteAcceptedInput,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.promote_accepted_input(promotion))
        .unwrap();
    store.execute(command)
}

fn mutation_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic contributor rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

#[test]
fn promotion_fault_cuts_reconcile_to_durable_prior_or_exact_across_reopen() {
    for (cut_name, point, required) in [
        (
            "before-commit",
            FaultPoint::BeforeCommit,
            Some(AcceptedInputPromotionStatus::Prior),
        ),
        (
            "after-commit-before-persist",
            FaultPoint::AfterCommitBeforePersist,
            None,
        ),
        (
            "after-persist",
            FaultPoint::AfterPersist,
            Some(AcceptedInputPromotionStatus::Exact),
        ),
    ] {
        let home = TestHome::new(&format!("phase58-promotion-fault-{cut_name}"));
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let fixture = promotion_fixture(90, id(90));
        let root_history = seed_detached_draft_backing(
            &store,
            storage,
            SyndicThreadId::from_bytes([0xf2; 16]),
            fixture.current_draft,
        );
        let mut records = fixture.records;
        install_current_draft_root_history(&mut records, fixture.current_draft, root_history);
        commit(&store, storage, batch(records));
        let request = promotion(
            &store,
            storage,
            SyndicTurnId::from_bytes([120; 16]),
            SyndicItemId::from_bytes([121; 16]),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.promote_accepted_input(request.clone()))
            .unwrap();

        faults.fail_next(point);
        let reconciliation = match (point, store.execute(command)) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { evidence }) => {
                assert!(matches!(evidence, CommandError::Commit { .. }));
                None
            }
            (
                FaultPoint::AfterCommitBeforePersist,
                CommandOutcome::Indeterminate {
                    failure,
                    reconciliation,
                },
            ) => {
                assert!(matches!(failure, CommandError::Persistence { .. }));
                Some(reconciliation)
            }
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    later_failure: Some(CommandError::Persistence { .. }),
                    ..
                },
            ) => None,
            (_, outcome) => panic!("unexpected promotion fault outcome: {outcome:?}"),
        };
        let (store, storage) = if let Some(reconciliation) = reconciliation {
            reconciliation.install();
            assert_eq!(store.health().state(), HomeHealthState::Healthy);
            (store, storage)
        } else {
            assert_eq!(store.health().state(), HomeHealthState::Failed);
            let recovery = store.recover_same_home().unwrap();
            let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            let store = recovery.publish();
            assert_eq!(store.health().state(), HomeHealthState::Healthy);
            (store, storage)
        };
        let recovered = storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap();
        assert!(
            matches!(
                recovered,
                AcceptedInputPromotionStatus::Prior | AcceptedInputPromotionStatus::Exact
            ),
            "a persistence cut must recover one recognized whole promotion state",
        );
        if let Some(required) = required {
            assert_eq!(recovered, required);
        }
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        if point == FaultPoint::AfterCommitBeforePersist {
            let close_error = store
                .close()
                .expect_err("installed indeterminate custody must block orderly close");
            assert_eq!(close_error.pending_reconciliation_scopes(), Some(1));
            drop(close_error);
            continue;
        }
        store.close().unwrap();

        let mut reopened = open(home.path());
        let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
        assert_eq!(
            reopened_storage
                .accepted_input_promotion_status(&reopened, &request, limit())
                .unwrap(),
            recovered,
        );
        reopened
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        reopened.close().unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum AuthorityDrift {
    Gate,
    Generation,
    Binding,
    DraftReverse,
    Thread,
}

impl AuthorityDrift {
    const ALL: [Self; 5] = [
        Self::Gate,
        Self::Generation,
        Self::Binding,
        Self::DraftReverse,
        Self::Thread,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Gate => "gate",
            Self::Generation => "generation",
            Self::Binding => "binding",
            Self::DraftReverse => "draft-reverse",
            Self::Thread => "thread",
        }
    }
}

fn substitute_exact_authority(
    records: &mut [FixtureRecord],
    thread: SyndicThreadId,
    drift: AuthorityDrift,
) {
    match drift {
        AuthorityDrift::Gate => {
            let gate = records
                .iter_mut()
                .find_map(|record| match record {
                    FixtureRecord::InputGate(gate) if gate.thread_id() == thread => Some(gate),
                    _ => None,
                })
                .unwrap();
            *gate = InputGateRecord::new(
                gate.thread_id(),
                gate.revision().checked_next().unwrap(),
                gate.state().clone(),
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                gate.selected_route(),
                gate.live_steering_count(),
                gate.live_next_turn_count(),
                gate.live_logical_utf8_bytes(),
            )
            .unwrap();
        }
        AuthorityDrift::Generation => {
            let revised = {
                let generation = records
                    .iter_mut()
                    .find_map(|record| match record {
                        FixtureRecord::AcceptedRouteGeneration(generation)
                            if generation.thread_id() == thread =>
                        {
                            Some(generation)
                        }
                        _ => None,
                    })
                    .unwrap();
                let revised = generation.revision().checked_next().unwrap();
                *generation = AcceptedRouteGenerationRecord::new(
                    generation.thread_id(),
                    generation.generation(),
                    revised,
                    generation.target().clone(),
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
                revised
            };
            let source = records
                .iter_mut()
                .find_map(|record| match record {
                    FixtureRecord::AcceptedNextSource(source) if source.thread_id() == thread => {
                        Some(source)
                    }
                    _ => None,
                })
                .unwrap();
            *source = AcceptedNextSourceRecord::new(
                source.thread_id(),
                source.generation(),
                revised,
                source.first_ordinal(),
                source.last_ordinal(),
            );
        }
        AuthorityDrift::Binding => {
            let binding = records
                .iter_mut()
                .find_map(|record| match record {
                    FixtureRecord::Binding(binding) if binding.thread_id() == thread => {
                        Some(binding)
                    }
                    _ => None,
                })
                .unwrap();
            *binding = BindingRecord::new(
                binding.thread_id(),
                binding.revision(),
                binding.selected_path(),
                BindingState::unbound("same-revision promotion fence drift").unwrap(),
            );
        }
        AuthorityDrift::DraftReverse => {
            let revised = {
                let draft = records
                    .iter_mut()
                    .find_map(|record| match record {
                        FixtureRecord::Draft(draft) if draft.thread_id() == thread => Some(draft),
                        _ => None,
                    })
                    .unwrap();
                let revised = draft.revision().checked_next().unwrap();
                *draft = DraftRecord::new(
                    draft.id(),
                    draft.thread_id(),
                    revised,
                    draft.submission_intent(),
                    draft.root_history(),
                    draft.created_at(),
                    draft.updated_at(),
                );
                revised
            };
            let reverse = records
                .iter_mut()
                .find_map(|record| match record {
                    FixtureRecord::DraftByThread(reverse) if reverse.thread_id() == thread => {
                        Some(reverse)
                    }
                    _ => None,
                })
                .unwrap();
            *reverse = DraftByThreadRecord::new(
                reverse.thread_id(),
                reverse.draft_id(),
                revised,
                reverse.thread_revision(),
            );
        }
        AuthorityDrift::Thread => {
            let replacement = beryl_model::SyndicDraftId::from_bytes([250; 16]);
            for record in records {
                match record {
                    FixtureRecord::Thread(current) if current.id() == thread => {
                        *current = ThreadRecord::new(
                            current.id(),
                            current.selected_path(),
                            replacement,
                            current.lineage(),
                            current.context_owner_id(),
                        );
                    }
                    FixtureRecord::Draft(draft) if draft.thread_id() == thread => {
                        *draft = DraftRecord::new(
                            replacement,
                            draft.thread_id(),
                            draft.revision(),
                            draft.submission_intent(),
                            draft.root_history(),
                            draft.created_at(),
                            draft.updated_at(),
                        );
                    }
                    FixtureRecord::DraftByThread(reverse) if reverse.thread_id() == thread => {
                        *reverse = DraftByThreadRecord::new(
                            reverse.thread_id(),
                            replacement,
                            reverse.draft_revision(),
                            reverse.thread_revision(),
                        );
                    }
                    FixtureRecord::AcceptedInput(input) if input.thread_id() == thread => {
                        let admission = input.admission();
                        *input = AcceptedInputRecord::new(
                            input.id(),
                            input.thread_id(),
                            input.ordinal(),
                            AcceptedInputAdmissionProof::new(
                                admission.expected_thread_revision(),
                                admission.source_draft_id(),
                                admission.expected_draft_revision(),
                                admission.expected_gate_revision(),
                                replacement,
                            )
                            .unwrap(),
                            input.route_generation(),
                            input.content(),
                            input.asset_reference_set(),
                            input.admitted_at(),
                        )
                        .unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
}

#[test]
fn same_domain_revision_still_fences_every_exact_promotion_authority() {
    let fixture = promotion_fixture(92, id(92));
    let (_source_home, source_store, source_storage) = seed(
        "phase58-promotion-authority-source",
        fixture.records.clone(),
    );
    source_store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let request = PromoteAcceptedInput::new(
        candidate(&source_store, source_storage),
        SyndicTurnId::from_bytes([140; 16]),
        SyndicItemId::from_bytes([141; 16]),
        timestamp(20),
    );

    for drift in AuthorityDrift::ALL {
        let mut records = fixture.records.clone();
        substitute_exact_authority(&mut records, fixture.thread, drift);
        let name = format!("phase58-promotion-authority-{}", drift.name());
        let (_home, store, storage) = seed(&name, records);
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        assert_eq!(
            storage.revision(&store).unwrap(),
            request.source_revision(),
            "the fixture must bypass only coarse domain-revision staleness",
        );
        assert_eq!(
            storage
                .accepted_input_promotion_status(&store, &request, limit())
                .unwrap(),
            AcceptedInputPromotionStatus::Collision,
            "{} substitution must not preserve Prior",
            drift.name(),
        );
        let error = match execute_promotion(&store, storage, request.clone()) {
            CommandOutcome::NotCommitted { evidence } => evidence,
            outcome => panic!("expected definitive promotion conflict, got {outcome:?}"),
        };
        assert!(
            matches!(
                mutation_error(&error),
                SyndicMutationError::AcceptedInputPromotionConflict
            ),
            "{} substitution returned {error}",
            drift.name(),
        );
        store.close().unwrap();
    }
    source_store.close().unwrap();
}
