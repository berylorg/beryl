use super::*;

pub(super) fn deletion_cases() -> Vec<DeletionCase> {
    vec![
        DeletionCase {
            family: PhysicalFamily::TurnItems,
            delete: FixtureDelete::TurnItem {
                turn: source_turn(),
                ordinal: TurnItemOrdinal::FIRST,
            },
            expected: "turn-item index is missing",
        },
        DeletionCase {
            family: PhysicalFamily::ActivityQueryEntries,
            delete: FixtureDelete::ActivityQueryEntry {
                thread: id(40),
                work_period: ActivityWorkPeriod::FIRST,
                order: ActivityQueryOrder::new(false, support::timestamp(1), activity_item()),
            },
            expected: "activity-query head counters or retention cutoff disagree",
        },
        DeletionCase {
            family: PhysicalFamily::ActivityQuerySources,
            delete: FixtureDelete::ActivityQuerySource {
                thread: id(40),
                work_period: ActivityWorkPeriod::FIRST,
                source_thread: id(40),
                source_turn: active_turn(),
            },
            expected: "activity-query source authority disagrees",
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
    ]
}
