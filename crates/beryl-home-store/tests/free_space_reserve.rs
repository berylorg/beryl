#![cfg(feature = "test-faults")]

use beryl_home_store::{
    CheckedBatchFootprint, DomainReader, DomainSchemaVersion, DurableStartFootprint,
    FreeSpaceOutcome, HomeOpenOptions, HomeSchemaVersion, HomeStore, MinimumTurnCaptureReserve,
    RecordFamily, StorageDomain, SyndicDurableStartFootprint, TurnStartAdmissionRequirement,
    participating_domain_footprint,
    test_faults::{FaultController, FreeSpaceTestObservation},
};
use tempfile::{TempDir, tempdir};

struct OpenedHome {
    _directory: TempDir,
    store: HomeStore,
}

struct SyndicTestDomain;

impl StorageDomain for SyndicTestDomain {
    const NAME: &'static str = "syndic";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[];
    type ValidationError = std::convert::Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

fn footprints(mutation: CheckedBatchFootprint) -> (DurableStartFootprint, DurableStartFootprint) {
    let metadata = participating_domain_footprint::<SyndicTestDomain>().unwrap();
    let direct = DurableStartFootprint::compose(
        SyndicDurableStartFootprint::idle_submission(mutation, metadata).unwrap(),
        None,
    )
    .unwrap();
    let queued = DurableStartFootprint::compose(
        SyndicDurableStartFootprint::accepted_input_promotion(mutation, metadata).unwrap(),
        None,
    )
    .unwrap();
    (direct, queued)
}

fn requirement(capture_reserve_bytes: u64) -> TurnStartAdmissionRequirement {
    let (direct, queued) = footprints(CheckedBatchFootprint::new(1, 1, 1));
    TurnStartAdmissionRequirement::try_new(
        direct,
        queued,
        MinimumTurnCaptureReserve::try_new(capture_reserve_bytes).unwrap(),
    )
    .unwrap()
}

#[test]
fn requirement_enforces_the_exact_fixed_policy_without_an_arbitrary_total_constructor() {
    let requirement = requirement(1);

    assert_eq!(
        beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES,
        256 * 1024 * 1024
    );
    assert_eq!(
        requirement.durable_start_budget_bytes(),
        beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES
    );
    assert!(
        requirement.direct_journal_append_bytes()
            <= beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES
    );
    assert!(
        requirement.queued_journal_append_bytes()
            <= beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES
    );
    assert_eq!(requirement.minimum_turn_capture_reserve().get(), 1);
    assert_eq!(
        requirement.total_bytes(),
        beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES + 1
    );
}

#[test]
fn requirement_rejects_zero_overflow_and_each_owner_path_drift() {
    assert_eq!(
        MinimumTurnCaptureReserve::try_new(0),
        Err(beryl_home_store::TurnStartAdmissionRequirementError::ZeroMinimumTurnCaptureReserve)
    );

    let (small_direct, small_queued) = footprints(CheckedBatchFootprint::new(1, 1, 1));
    assert_eq!(
        TurnStartAdmissionRequirement::try_new(
            small_direct,
            small_queued,
            MinimumTurnCaptureReserve::try_new(u64::MAX).unwrap(),
        ),
        Err(
            beryl_home_store::TurnStartAdmissionRequirementError::ArithmeticOverflow {
                budget_bytes: beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES,
                capture_reserve_bytes: u64::MAX,
            }
        )
    );

    let (large_direct, large_queued) = footprints(CheckedBatchFootprint::new(
        1,
        beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES,
        0,
    ));
    assert_eq!(
        TurnStartAdmissionRequirement::try_new(
            large_direct,
            small_queued,
            MinimumTurnCaptureReserve::try_new(1).unwrap(),
        ),
        Err(
            beryl_home_store::TurnStartAdmissionRequirementError::DirectDurableStartBudgetDrift {
                journal_append_bytes: large_direct.journal_append_bytes(),
                budget_bytes: beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES,
            }
        )
    );
    assert_eq!(
        TurnStartAdmissionRequirement::try_new(
            small_direct,
            large_queued,
            MinimumTurnCaptureReserve::try_new(1).unwrap(),
        ),
        Err(
            beryl_home_store::TurnStartAdmissionRequirementError::QueuedDurableStartBudgetDrift {
                journal_append_bytes: large_queued.journal_append_bytes(),
                budget_bytes: beryl_home_store::DURABLE_START_ADMISSION_BUDGET_BYTES,
            }
        )
    );
}

fn open(faults: FaultController) -> OpenedHome {
    let directory = tempdir().expect("temporary home directory");
    let store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .expect("open home with free-space test seam");
    OpenedHome {
        _directory: directory,
        store,
    }
}

#[test]
fn reserve_query_reports_each_tested_physical_boundary_outcome_exactly() {
    let faults = FaultController::new();
    let home = open(faults.clone());
    let requirement = requirement(1);
    let reserve_bytes = requirement.total_bytes();
    let observations = [
        FreeSpaceTestObservation::Observed {
            available_bytes: reserve_bytes + 4096,
            total_free_bytes: reserve_bytes + 4096,
            total_bytes: reserve_bytes + 8192,
        },
        FreeSpaceTestObservation::Observed {
            available_bytes: reserve_bytes - 1,
            total_free_bytes: reserve_bytes - 1,
            total_bytes: reserve_bytes + 8192,
        },
        FreeSpaceTestObservation::Unavailable,
        FreeSpaceTestObservation::Observed {
            available_bytes: reserve_bytes + 4096,
            total_free_bytes: reserve_bytes,
            total_bytes: reserve_bytes + 8192,
        },
    ];
    for observation in observations {
        faults.push_free_space_observation(observation);
    }

    assert_eq!(
        home.store.query_free_space(requirement),
        FreeSpaceOutcome::Sufficient {
            available_bytes: reserve_bytes + 4096,
            reserve_bytes,
        }
    );
    assert_eq!(faults.free_space_observation_count(), 1);
    assert_eq!(
        home.store.query_free_space(requirement),
        FreeSpaceOutcome::BelowReserve {
            available_bytes: reserve_bytes - 1,
            reserve_bytes,
        }
    );
    assert_eq!(faults.free_space_observation_count(), 2);
    assert_eq!(
        home.store.query_free_space(requirement),
        FreeSpaceOutcome::Unavailable
    );
    assert_eq!(faults.free_space_observation_count(), 3);
    assert_eq!(
        home.store.query_free_space(requirement),
        FreeSpaceOutcome::Indeterminate
    );
    assert_eq!(faults.free_space_observation_count(), 4);
}

#[test]
fn repeated_queries_consume_independent_observations_without_caching() {
    let faults = FaultController::new();
    let home = open(faults.clone());
    let requirement = requirement(10);
    let reserve_bytes = requirement.total_bytes();
    faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
        available_bytes: reserve_bytes + 2,
        total_free_bytes: reserve_bytes + 2,
        total_bytes: reserve_bytes + 10,
    });
    faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
        available_bytes: reserve_bytes - 1,
        total_free_bytes: reserve_bytes - 1,
        total_bytes: reserve_bytes + 10,
    });

    assert_eq!(
        home.store.query_free_space(requirement),
        FreeSpaceOutcome::Sufficient {
            available_bytes: reserve_bytes + 2,
            reserve_bytes,
        }
    );
    assert_eq!(
        home.store.query_free_space(requirement),
        FreeSpaceOutcome::BelowReserve {
            available_bytes: reserve_bytes - 1,
            reserve_bytes,
        }
    );
    assert_eq!(faults.free_space_observation_count(), 2);
}
