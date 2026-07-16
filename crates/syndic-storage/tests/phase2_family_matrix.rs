#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{DomainRegistrationError, DomainValidationError, HomeRecoveryError};
use beryl_model::{BindingRevision, DiscussionContextOwnerId, SyndicTurnId};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord, PhysicalFamily};
use syndic_storage::*;

use support::{
    TestHome, batch, commit, draft_id, id, open,
    populated::{
        active_item, active_snapshot, active_turn, build_item, cas_item, cas_thread, cas_turn,
        next_input, populated_records, source_item, source_projection, source_resource,
        source_resource_projection, source_turn, steering_input, suffix_item,
    },
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

fn assert_registration_rejection(error: DomainRegistrationError, expected: &str) {
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected Syndic semantic registration rejection, got {other:?}"),
    }
}

fn assert_validation_rejection(error: DomainValidationError, expected: &str) {
    match error {
        DomainValidationError::Rejected { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected Syndic semantic validation rejection, got {other:?}"),
    }
}

fn exercise_deletion(case: DeletionCase) {
    let registration_home = TestHome::new(&format!("delete-{}-registration", case.family.name()));
    let mut store = open(registration_home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    store.validate_registered_domains().unwrap();
    commit(&store, storage, deletion_batch(case.delete.clone()));
    store.close().unwrap();

    let mut reopened = open(registration_home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("{} deletion reopened successfully", case.family.name()),
        Err(error) => error,
    };
    assert_registration_rejection(error, case.expected);
    reopened.close().unwrap();

    let recovery_home = TestHome::new(&format!("delete-{}-recovery", case.family.name()));
    let mut store = open(recovery_home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    store.validate_registered_domains().unwrap();
    commit(&store, storage, deletion_batch(case.delete));
    assert_validation_rejection(
        store.validate_registered_domains().unwrap_err(),
        case.expected,
    );
    match store.recover_same_home().unwrap_err() {
        HomeRecoveryError::DomainValidation(error) => {
            assert_validation_rejection(error, case.expected);
        }
        other => panic!("expected Syndic recovery rejection, got {other:?}"),
    }
    store.close().unwrap();
}

fn exercise_accepted_deletion(family: PhysicalFamily, delete: FixtureDelete) {
    let home = TestHome::new(&format!("delete-{}-accepted", family.name()));
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    commit(&store, storage, deletion_batch(delete));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn populated_fixture_covers_every_family_and_reopens_cleanly() {
    let records = populated_records();
    assert_eq!(PhysicalFamily::ALL.len(), 44);
    for family in PhysicalFamily::ALL {
        assert!(
            records.iter().any(|record| record.family() == family),
            "fixture omitted {}",
            family.name()
        );
    }

    let home = TestHome::new("populated-family-matrix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(records));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn reverse_index_getters_expose_every_stored_correlation() {
    let mut seen = [false; 8];
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
            FixtureRecord::AcceptedSteering(record) => {
                assert_eq!(record.thread_id(), id(40));
                assert_eq!(record.turn_id(), active_turn());
                assert_eq!(record.ordinal(), AcceptedInputOrdinal::FIRST);
                assert_eq!(record.input_id(), steering_input());
                assert_eq!(record.input_revision().get(), 1);
                seen[2] = true;
            }
            FixtureRecord::CasItem(record) if record.item_id() == active_item() => {
                assert_eq!(record.cas_thread_id(), &cas_thread());
                assert_eq!(record.cas_turn_id(), &cas_turn());
                assert_eq!(record.cas_item_id(), &cas_item());
                assert_eq!(record.item_id(), active_item());
                assert_eq!(record.item_revision().get(), 1);
                seen[3] = true;
            }
            FixtureRecord::BindingHead(record) if record.thread_id() == id(40) => {
                assert_eq!(record.revision().get(), 3);
                assert_eq!(record.lifecycle(), BindingLifecycle::Active);
                assert_eq!(
                    record.selected_path_digest(),
                    root_turn_chain_digest(active_turn())
                );
                seen[4] = true;
            }
            FixtureRecord::CasThread(record) if record.thread_id() == id(40) => {
                assert_eq!(record.cas_thread_id(), &cas_thread());
                assert_eq!(record.thread_id(), id(40));
                assert_eq!(record.first_binding_revision().get(), 2);
                assert_eq!(record.latest_binding_revision().get(), 3);
                seen[5] = true;
            }
            FixtureRecord::CasThreadBinding(record) if record.thread_id() == id(40) => {
                assert_eq!(record.cas_thread_id(), &cas_thread());
                assert_eq!(record.thread_id(), id(40));
                assert!([2, 3].contains(&record.binding_revision().get()));
                seen[6] = true;
            }
            FixtureRecord::CasTurn(record) if record.turn_id() == active_turn() => {
                assert_eq!(record.cas_thread_id(), &cas_thread());
                assert_eq!(record.cas_turn_id(), &cas_turn());
                assert_eq!(record.thread_id(), id(40));
                assert_eq!(record.turn_id(), active_turn());
                assert_eq!(record.binding_revision().get(), 3);
                assert_eq!(record.snapshot_id(), active_snapshot());
                seen[7] = true;
            }
            _ => {}
        }
    }
    assert!(seen.into_iter().all(|value| value));
}

#[test]
fn every_family_has_an_exact_deletion_case_with_explicit_semantic_outcome() {
    let cases = deletion_cases();
    let rejection_families: Vec<_> = PhysicalFamily::ALL
        .into_iter()
        .filter(|family| *family != PhysicalFamily::ItemProjectionBuilds)
        .collect();
    assert_eq!(cases.len(), rejection_families.len());
    for (case, family) in cases.into_iter().zip(rejection_families) {
        assert_eq!(case.family, family);
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
            family: PhysicalFamily::InputMarkerResolutions,
            delete: FixtureDelete::InputMarkerResolution {
                owner: InputMarkerOwner::AcceptedInput(steering_input()),
                ordinal: InputMarkerOrdinal::FIRST,
            },
            expected: "input marker zero frontier disagrees",
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
            expected: "thread input gate is missing",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedInputs,
            delete: FixtureDelete::AcceptedInput(next_input()),
            expected: "accepted-order target is missing",
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
            expected: "turn-item target is missing",
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
            expected: "finalized visible turn item has no projection set",
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
            expected: "current transcript head build manifest is missing",
        },
        DeletionCase {
            family: PhysicalFamily::Projections,
            delete: FixtureDelete::Projection(source_projection()),
            expected: "stable item-projection target is missing",
        },
    ];
    cases.extend([
        DeletionCase {
            family: PhysicalFamily::Resources,
            delete: FixtureDelete::Resource(source_resource()),
            expected: "projection resource metadata is missing",
        },
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
            expected: "steering input gate execution snapshot is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ActiveCasTurns,
            delete: FixtureDelete::ActiveCasTurn(active_snapshot()),
            expected: "steerable gate active CAS turn is missing",
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
            expected: "accepted-input order index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedSteering,
            delete: FixtureDelete::AcceptedSteering {
                thread: id(40),
                turn: active_turn(),
                ordinal: AcceptedInputOrdinal::FIRST,
            },
            expected: "accepted steering index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::AcceptedNextTurn,
            delete: FixtureDelete::AcceptedNextTurn {
                thread: id(40),
                ordinal: AcceptedInputOrdinal::new(2).unwrap(),
            },
            expected: "accepted next-turn index is missing",
        },
    ]);
    cases.extend([
        DeletionCase {
            family: PhysicalFamily::TurnItems,
            delete: FixtureDelete::TurnItem {
                turn: source_turn(),
                ordinal: TurnItemOrdinal::FIRST,
            },
            expected: "turn-item index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ItemSourceEvents,
            delete: FixtureDelete::ItemSourceEvent {
                item: active_item(),
                ordinal: ItemSourceEventOrdinal::FIRST,
            },
            expected: "canonical item source-event index presence disagrees",
        },
        DeletionCase {
            family: PhysicalFamily::CasItem,
            delete: FixtureDelete::CasItem {
                thread: cas_thread(),
                turn: cas_turn(),
                item: cas_item(),
            },
            expected: "CAS item reverse index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::TranscriptPathTurns,
            delete: FixtureDelete::TranscriptPathTurn {
                thread: id(30),
                generation: TranscriptGeneration::FIRST,
                depth: TurnDepth::FIRST,
            },
            expected: "transcript build first collected path record is missing",
        },
        DeletionCase {
            family: PhysicalFamily::TranscriptViewEntries,
            delete: FixtureDelete::TranscriptViewEntry {
                thread: id(30),
                generation: TranscriptGeneration::FIRST,
                position: TranscriptPosition::FIRST,
            },
            expected: "transcript head zero frontier disagrees",
        },
        DeletionCase {
            family: PhysicalFamily::StableItemProjections,
            delete: FixtureDelete::StableItemProjection {
                item: source_item(),
                ordinal: ProjectionOrdinal::FIRST,
            },
            expected: "replayed stable projection membership is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ItemProjections,
            delete: FixtureDelete::ItemProjection {
                item: suffix_item(),
                generation: ItemProjectionGeneration::FIRST,
                ordinal: ProjectionOrdinal::FIRST,
            },
            expected: "replayed projection suffix membership is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ProjectionResources,
            delete: FixtureDelete::ProjectionResource {
                projection: source_resource_projection(),
                ordinal: ResourceOrdinal::FIRST,
            },
            expected: "projection-resource index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::BindingHeads,
            delete: FixtureDelete::BindingHead(id(30)),
            expected: "thread binding head is missing",
        },
        DeletionCase {
            family: PhysicalFamily::CasThread,
            delete: FixtureDelete::CasThread(cas_thread()),
            expected: "CAS thread reservation is missing",
        },
        DeletionCase {
            family: PhysicalFamily::CasThreadBinding,
            delete: FixtureDelete::CasThreadBinding {
                thread: cas_thread(),
                revision: BindingRevision::new(3).unwrap(),
            },
            expected: "CAS thread binding membership is missing",
        },
        DeletionCase {
            family: PhysicalFamily::CasTurn,
            delete: FixtureDelete::CasTurn {
                thread: cas_thread(),
                turn: cas_turn(),
            },
            expected: "source event CAS-turn index is missing",
        },
    ]);
    cases
}
