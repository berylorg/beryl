#![cfg(feature = "test-faults")]

mod support;

#[path = "phase2_family_matrix/cases_tail.rs"]
mod cases_tail;

use beryl_home_store::{DomainRegistrationError, DomainValidationError};
use beryl_model::{BindingRevision, DiscussionContextOwnerId, SyndicTurnId};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord, PhysicalFamily};
use syndic_storage::*;

use support::{
    TestHome, commit, draft_id, id, open,
    populated::{
        active_item, active_snapshot, active_turn, activity_item, build_item, cas_item, cas_thread,
        cas_turn, next_input, populated_records, source_item, source_projection, source_turn,
        steering_input, suffix_item,
    },
    seed_populated,
};

struct DeletionCase {
    family: PhysicalFamily,
    delete: FixtureDelete,
    expected: &'static str,
}

fn deletion_batch(delete: FixtureDelete) -> FixtureBatch {
    let mut batch = FixtureBatch::new();
    batch.delete(delete).unwrap();
    batch
}

/// Resolves collection rows from the seeded current transcript head. Those records are produced
/// by the command lifecycle, so their generation must not be copied from the former synthetic
/// aggregate.
fn seeded_delete(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    delete: &FixtureDelete,
) -> FixtureDelete {
    let current_generation = |thread| {
        storage
            .transcript_view_head(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
            .unwrap()
            .unwrap_or_else(|| panic!("seeded transcript head disappeared"))
            .generation()
    };
    match delete {
        FixtureDelete::TranscriptPathTurn { thread, depth, .. } => {
            FixtureDelete::TranscriptPathTurn {
                thread: *thread,
                generation: current_generation(*thread),
                depth: *depth,
            }
        }
        FixtureDelete::TranscriptViewEntry {
            thread, position, ..
        } => FixtureDelete::TranscriptViewEntry {
            thread: *thread,
            generation: current_generation(*thread),
            position: *position,
        },
        _ => delete.clone(),
    }
}

fn assert_registration_rejection(error: DomainRegistrationError, expected: &str) {
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected Syndic semantic registration rejection, got {other:?}"),
    }
}

fn assert_validation_rejection(error: &DomainValidationError, expected: &str) {
    match error {
        DomainValidationError::Rejected { domain, source } => {
            assert_eq!(*domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected Syndic semantic validation rejection, got {other:?}"),
    }
}

fn exercise_deletion(case: DeletionCase) {
    let registration_home = TestHome::new(&format!("delete-{}-registration", case.family.name()));
    let mut store = open(registration_home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(
        &store,
        storage,
        deletion_batch(seeded_delete(&store, storage, &case.delete)),
    );
    store.close().unwrap();

    let mut reopened = open(registration_home.path());
    let error = match SyndicStorage::register_with_schema_validation(&mut reopened) {
        Ok(_) => panic!("{} deletion reopened successfully", case.family.name()),
        Err(error) => error,
    };
    assert_registration_rejection(error, case.expected);
    reopened.close().unwrap();

    let recovery_home = TestHome::new(&format!("delete-{}-recovery", case.family.name()));
    let mut store = open(recovery_home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(
        &store,
        storage,
        deletion_batch(seeded_delete(&store, storage, &case.delete)),
    );
    assert_validation_rejection(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .validation_error(),
        case.expected,
    );
    let candidate = store.recover_same_home().unwrap();
    SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let recovered = candidate.publish();
    SyndicStorage::reacquire(&recovered).unwrap();
    recovered.close().unwrap();
}

fn exercise_accepted_deletion(family: PhysicalFamily, delete: FixtureDelete) {
    let home = TestHome::new(&format!("delete-{}-accepted", family.name()));
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    commit(&store, storage, deletion_batch(delete));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn populated_fixture_covers_every_resting_family_and_reopens_cleanly() {
    let records = populated_records();
    assert_eq!(PhysicalFamily::ALL.len(), 61);
    // Provider staging, stop-operation, and compaction families are covered by their dedicated
    // phase fixtures rather than this legacy populated aggregate. Resource families are
    // intentionally unrepresented because the fixture's plain provider text produces no typed
    // code or table resources. Strict physical corruption still covers every registered codec.
    for family in PhysicalFamily::ALL.into_iter().filter(|family| {
        !matches!(
            family,
            PhysicalFamily::ProviderItemBuilds
                | PhysicalFamily::ProviderNarrativeSpans
                | PhysicalFamily::CanonicalItems
                | PhysicalFamily::TurnItems
                | PhysicalFamily::ItemSourceEvents
                | PhysicalFamily::CasItem
                | PhysicalFamily::ItemProjectionHeads
                | PhysicalFamily::ItemProjectionSets
                | PhysicalFamily::ItemProjectionBuilds
                | PhysicalFamily::StableItemProjections
                | PhysicalFamily::ItemProjections
                | PhysicalFamily::Projections
                | PhysicalFamily::Resources
                | PhysicalFamily::ProjectionResources
                | PhysicalFamily::TranscriptViewHeads
                | PhysicalFamily::TranscriptBuilds
                | PhysicalFamily::TranscriptPathTurns
                | PhysicalFamily::TranscriptViewEntries
                | PhysicalFamily::ActivityQueryHeads
                | PhysicalFamily::ActivityQueryEntries
                | PhysicalFamily::ActivityQuerySources
                | PhysicalFamily::InputGates
                | PhysicalFamily::AcceptedInputs
                | PhysicalFamily::AcceptedRouteGenerationHeads
                | PhysicalFamily::AcceptedRouteLeaves
                | PhysicalFamily::AcceptedOrder
                | PhysicalFamily::AcceptedRouteGenerations
                | PhysicalFamily::AcceptedReadySources
                | PhysicalFamily::AcceptedNextSources
                | PhysicalFamily::ImageLabelOriginSpans
                | PhysicalFamily::ProviderObservationBuilds
                | PhysicalFamily::ProviderObservationChunks
                | PhysicalFamily::StopOperations
                | PhysicalFamily::CompactionOperations
                | PhysicalFamily::CompactionSettlementReceipts
        )
    }) {
        assert!(
            records.iter().any(|record| record.family() == family),
            "fixture omitted {}",
            family.name()
        );
    }

    let home = TestHome::new("populated-family-matrix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn reverse_index_getters_expose_every_stored_correlation() {
    // The generic aggregate intentionally contains only non-provider fixture records.  Keep the
    // two static index facts here, then exercise the command-produced correlations through the
    // bounded readers that own them.
    let mut seen = [false; 2];
    for record in populated_records() {
        match record {
            FixtureRecord::DraftByThread(record) if record.thread_id() == id(30) => {
                assert_eq!(record.draft_id(), draft_id(31));
                assert_eq!(record.draft_revision().get(), 1);
                assert_eq!(record.thread_revision().get(), 1);
                seen[0] = true;
            }
            FixtureRecord::ThreadParent(record) => {
                assert_eq!(record.parent_thread_id(), id(30));
                assert_eq!(record.child_thread_id(), id(36));
                assert_eq!(record.child_revision().get(), 1);
                assert_eq!(
                    record.context_owner_id(),
                    DiscussionContextOwnerId::Draft(draft_id(37))
                );
                seen[1] = true;
            }
            _ => {}
        }
    }
    assert!(seen.into_iter().all(|value| value));

    let home = TestHome::new("reverse-index-getters");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();

    let capture = storage
        .capture_item(
            &store,
            &CasItemSource::new(CasTurnSource::new(cas_thread(), cas_turn()), cas_item()),
            limit,
        )
        .unwrap()
        .unwrap();
    assert_eq!(capture.cas_index().cas_thread_id(), &cas_thread());
    assert_eq!(capture.cas_index().cas_turn_id(), &cas_turn());
    assert_eq!(capture.cas_index().cas_item_id(), &cas_item());
    assert_eq!(capture.cas_index().item_id(), active_item());
    assert_eq!(capture.cas_index().item_revision().get(), 3);

    let binding = storage
        .current_binding(&store, id(40), limit)
        .unwrap()
        .unwrap();
    assert_eq!(binding.head().revision().get(), 3);
    assert_eq!(binding.head().lifecycle(), BindingLifecycle::Active);
    assert_eq!(
        binding.head().selected_path_digest(),
        root_turn_chain_digest(active_turn())
    );

    let cas_owner = storage
        .cas_thread_owner(&store, cas_thread(), limit)
        .unwrap()
        .unwrap();
    assert_eq!(cas_owner.cas_thread_id(), &cas_thread());
    assert_eq!(cas_owner.thread_id(), id(40));
    assert_eq!(cas_owner.first_binding_revision().get(), 2);
    assert_eq!(cas_owner.latest_binding_revision().get(), 3);

    for revision in [2, 3] {
        let membership = storage
            .fixture_cas_thread_binding_membership(
                &store,
                cas_thread(),
                BindingRevision::new(revision).unwrap(),
                limit,
            )
            .unwrap()
            .unwrap();
        assert_eq!(membership.cas_thread_id(), &cas_thread());
        assert_eq!(membership.thread_id(), id(40));
        assert_eq!(membership.binding_revision().get(), revision);
    }

    let turn_owner = storage
        .cas_turn_owner(&store, cas_thread(), cas_turn(), limit)
        .unwrap()
        .unwrap();
    assert_eq!(turn_owner.cas_thread_id(), &cas_thread());
    assert_eq!(turn_owner.cas_turn_id(), &cas_turn());
    assert_eq!(turn_owner.thread_id(), id(40));
    assert_eq!(turn_owner.turn_id(), active_turn());
    assert_eq!(turn_owner.binding_revision().get(), 3);
    assert_eq!(turn_owner.snapshot_id(), active_snapshot());
    store.close().unwrap();
}

#[test]
fn every_family_has_an_exact_deletion_case_with_explicit_semantic_outcome() {
    let cases = deletion_cases();
    // Provider staging, stop-operation, and compaction families have dedicated semantic coverage
    // and no row in this legacy populated aggregate. Resource families are also unrepresented:
    // plain provider text creates no typed code or table resources, so there is no deletion row to
    // accept. Strict physical corruption still covers every registered codec.
    let rejection_families: Vec<_> = PhysicalFamily::ALL
        .into_iter()
        .filter(|family| {
            !matches!(
                family,
                PhysicalFamily::ProviderItemBuilds
                    | PhysicalFamily::ProviderObservationBuilds
                    | PhysicalFamily::ProviderObservationChunks
                    | PhysicalFamily::ItemProjectionBuilds
                    | PhysicalFamily::Resources
                    | PhysicalFamily::ProjectionResources
                    | PhysicalFamily::StopOperations
                    | PhysicalFamily::CompactionOperations
                    | PhysicalFamily::CompactionSettlementReceipts
            )
        })
        .collect();
    assert_eq!(cases.len(), rejection_families.len());
    for family in rejection_families {
        assert_eq!(
            cases.iter().filter(|case| case.family == family).count(),
            1,
            "{} needs exactly one deletion case",
            family.name()
        );
    }
    for case in cases {
        exercise_deletion(case);
    }

    // An unselected resumable build is not authority. Losing it is a valid restart point,
    // while its exact physical family remains covered by strict codec-corruption fixtures.
    exercise_accepted_deletion(
        PhysicalFamily::ItemProjectionBuilds,
        FixtureDelete::ItemProjectionBuild {
            item: build_item(),
            generation: ItemProjectionGeneration::FIRST,
        },
    );
}

fn deletion_cases() -> Vec<DeletionCase> {
    let mut cases = vec![
        DeletionCase {
            family: PhysicalFamily::Threads,
            delete: FixtureDelete::Thread(id(40)),
            expected: "draft owner thread is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ThreadExecutions,
            delete: FixtureDelete::ThreadExecution(id(40)),
            expected: "thread execution record is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ThreadAttributes,
            delete: FixtureDelete::ThreadAttributes(id(40)),
            expected: "thread attributes record is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ThreadUsage,
            delete: FixtureDelete::ThreadUsage(id(40)),
            expected: "thread usage record is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ThreadCatalogSummaries,
            delete: FixtureDelete::ThreadCatalogSummary(id(40)),
            expected: "thread catalog summary is missing",
        },
        DeletionCase {
            family: PhysicalFamily::Drafts,
            delete: FixtureDelete::Draft(draft_id(41)),
            expected: "thread current draft is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ContentManifests,
            delete: FixtureDelete::ContentManifest(
                PreparedContent::utf8("assistant").unwrap().id(),
            ),
            expected: "content chunk owner manifest is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ContentChunks,
            delete: FixtureDelete::ContentChunk {
                content: PreparedContent::utf8("assistant").unwrap().id(),
                ordinal: ContentChunkOrdinal::FIRST,
            },
            expected: "content zero-chunk frontier disagrees",
        },
        DeletionCase {
            family: PhysicalFamily::ContentByteSpans,
            delete: FixtureDelete::ContentByteSpan {
                content: PreparedContent::utf8("assistant").unwrap().id(),
                start: 0,
            },
            expected: "content zero-span frontier disagrees",
        },
        DeletionCase {
            family: PhysicalFamily::ContentTextSpans,
            delete: FixtureDelete::ContentTextSpan {
                content: PreparedContent::utf8("assistant").unwrap().id(),
                logical_start: 0,
            },
            expected: "content text zero-span frontier disagrees",
        },
        DeletionCase {
            family: PhysicalFamily::ContentPieces,
            delete: FixtureDelete::ContentPiece {
                content: PreparedContent::utf8("assistant").unwrap().id(),
                ordinal: ContentPieceOrdinal::FIRST,
            },
            expected: "content text span piece is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ProviderNarrativeSpans,
            delete: FixtureDelete::ProviderNarrativeSpan {
                content: beryl_model::SyndicContentId::from_bytes(*source_item().as_bytes()),
                generation: ProviderNarrativeGeneration::FIRST,
                logical_start: 0,
            },
            expected: "published provider narrative span is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ContextEnvelopes,
            delete: FixtureDelete::ContextEnvelope(DiscussionContextOwnerId::Draft(draft_id(37))),
            expected: "thread context envelope is missing",
        },
        DeletionCase {
            family: PhysicalFamily::Turns,
            delete: FixtureDelete::Turn(active_turn()),
            expected: "thread committed tail is missing",
        },
        DeletionCase {
            family: PhysicalFamily::TurnStates,
            delete: FixtureDelete::TurnState(active_turn()),
            expected: "turn state is missing",
        },
        DeletionCase {
            family: PhysicalFamily::InputGates,
            delete: FixtureDelete::InputGate(id(40)),
            expected: "accepted input references a missing input gate",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedInputs,
            delete: FixtureDelete::AcceptedInput(next_input()),
            expected: "accepted-input replacement descendant is not exclusive",
        },
        DeletionCase {
            family: PhysicalFamily::SourceEvents,
            delete: FixtureDelete::SourceEvent {
                turn: active_turn(),
                sequence: SourceEventSequence::FIRST,
            },
            expected: "source-event key or contiguous sequence disagrees",
        },
        DeletionCase {
            family: PhysicalFamily::CanonicalItems,
            delete: FixtureDelete::CanonicalItem(source_item()),
            expected: "activity-query visible source item is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ActivityQueryHeads,
            delete: FixtureDelete::ActivityQueryHead(id(40)),
            expected: "thread activity-query head is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ItemProjectionHeads,
            delete: FixtureDelete::ItemProjectionHead(source_item()),
            expected: "finalized visible turn item has no projection head",
        },
        DeletionCase {
            family: PhysicalFamily::ItemProjectionSets,
            delete: FixtureDelete::ItemProjectionSet {
                item: source_item(),
                generation: ItemProjectionGeneration::FIRST,
            },
            expected: "item projection generation sequence has a gap",
        },
        DeletionCase {
            family: PhysicalFamily::TranscriptViewHeads,
            delete: FixtureDelete::TranscriptViewHead(id(30)),
            expected: "thread transcript head is missing",
        },
        DeletionCase {
            family: PhysicalFamily::TranscriptBuilds,
            delete: FixtureDelete::TranscriptBuild {
                thread: id(30),
                generation: TranscriptGeneration::FIRST,
            },
            expected: "transcript path build owner is missing",
        },
        DeletionCase {
            family: PhysicalFamily::Projections,
            delete: FixtureDelete::Projection(source_projection()),
            expected: "stable item-projection target is missing",
        },
    ];
    cases.extend([
        DeletionCase {
            family: PhysicalFamily::HistorySummaries,
            delete: FixtureDelete::HistorySummary(id(30)),
            expected: "thread history summary is missing",
        },
        DeletionCase {
            family: PhysicalFamily::Bindings,
            delete: FixtureDelete::Binding {
                thread: id(40),
                revision: BindingRevision::new(2).unwrap(),
            },
            expected: "CAS thread first binding is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ExecutionSnapshots,
            delete: FixtureDelete::ExecutionSnapshot(active_snapshot()),
            expected: "active binding snapshot is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ActiveCasTurns,
            delete: FixtureDelete::ActiveCasTurn(active_snapshot()),
            expected: "current active binding has uncorrelated source history",
        },
        DeletionCase {
            family: PhysicalFamily::DraftByThread,
            delete: FixtureDelete::DraftByThread(id(30)),
            expected: "thread draft reverse index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ThreadParent,
            delete: FixtureDelete::ThreadParent {
                parent: id(30),
                child: id(36),
            },
            expected: "thread parent index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ImageLabelOriginSpans,
            delete: FixtureDelete::ImageLabelOriginSpan {
                thread: id(40),
                end_label: ImageLabelOrdinal::FIRST,
            },
            expected: "thread final image-label origin span is missing",
        },
        DeletionCase {
            family: PhysicalFamily::TurnChildren,
            delete: FixtureDelete::TurnChild {
                parent: SyndicTurnId::from_bytes([29; 16]),
                child: source_turn(),
            },
            expected: "turn child index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedOrder,
            delete: FixtureDelete::AcceptedOrder {
                thread: id(40),
                ordinal: AcceptedInputOrdinal::new(2).unwrap(),
            },
            expected: "accepted input is missing immutable order membership",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedRouteLeaves,
            delete: FixtureDelete::AcceptedRouteLeaf(steering_input()),
            expected: "accepted input is missing its route leaf",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedRouteGenerations,
            delete: FixtureDelete::AcceptedRouteGeneration {
                thread: id(40),
                generation: AcceptedRouteGeneration::FIRST,
            },
            expected: "accepted input references a missing route generation",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedRouteGenerationHeads,
            delete: FixtureDelete::AcceptedRouteGenerationHead(id(40)),
            expected: "accepted-route generation owner is missing its route head",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedReadySources,
            delete: FixtureDelete::AcceptedReadySource {
                thread: id(40),
                generation: AcceptedRouteGeneration::FIRST,
            },
            expected: "accepted-route generation and ready-source authority disagree",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedNextSources,
            delete: FixtureDelete::AcceptedNextSource {
                thread: id(40),
                generation: AcceptedRouteGeneration::FIRST,
            },
            expected: "accepted-route generation and next-source presence disagree",
        },
    ]);
    cases.extend(cases_tail::deletion_cases());
    cases
}
