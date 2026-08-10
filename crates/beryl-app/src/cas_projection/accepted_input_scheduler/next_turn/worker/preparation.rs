use std::time::{SystemTime, UNIX_EPOCH};

use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AcceptedInputPromotionStatus, PromoteAcceptedInput, SelectedPathProof, SyndicReadError,
    SyndicTimestamp,
};

use super::super::{
    super::{SchedulerFailure, WorkerDisposition, failure},
    authority::{
        LeaseValidationAuthority, expected_coordinator_drift, obsolete_coordinator_generation,
    },
};
use crate::input_admission::accepted_input_promotion_status;

pub(super) fn classify_unbuilt_promotion(
    validator: &LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    promotion: &PromoteAcceptedInput,
) -> WorkerDisposition {
    if let Err(error) = validator.ensure_current() {
        pause_obsolete_generation(
            promotion.thread_id(),
            obsolete_coordinator_generation(&error),
        );
        return if failure::is_verification_pending_coordinator(&error, validator.home_generation())
        {
            WorkerDisposition::VerificationPending
        } else if failure::is_cut_correlated_coordinator(&error, validator.home_generation()) {
            WorkerDisposition::PersistentHomeFailure
        } else if expected_coordinator_drift(&error) {
            WorkerDisposition::NextParked
        } else {
            WorkerDisposition::Fatal
        };
    }
    match storage.accepted_input_promotion_status(
        &validator.home,
        promotion,
        crate::cas_projection::input_replay::point_limit(),
    ) {
        Ok(AcceptedInputPromotionStatus::Collision)
        | Err(SyndicReadError::ConcurrentChange { .. }) => WorkerDisposition::NextContinue,
        Err(error) => match failure::from_syndic_read(&error, validator.home_generation()) {
            SchedulerFailure::VerificationPending => WorkerDisposition::VerificationPending,
            SchedulerFailure::PersistentHomeFailure => WorkerDisposition::PersistentHomeFailure,
            SchedulerFailure::Fatal => WorkerDisposition::Fatal,
        },
        Ok(AcceptedInputPromotionStatus::Prior | AcceptedInputPromotionStatus::Exact) => {
            WorkerDisposition::Fatal
        }
    }
}

pub(super) fn pause_obsolete_generation(thread_id: SyndicThreadId, obsolete: bool) {
    #[cfg(feature = "test-faults")]
    if obsolete {
        crate::cas_projection::test_faults::pause_scheduled_generation_invalidation(thread_id);
    }
    #[cfg(not(feature = "test-faults"))]
    let _ = (thread_id, obsolete);
}

pub(super) fn reconcile_promotion(
    validator: &LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    assets: beryl_state::AssetState,
    promotion: &PromoteAcceptedInput,
) -> Result<AcceptedInputPromotionStatus, SchedulerFailure> {
    let read = || {
        accepted_input_promotion_status(
            &validator.home,
            storage,
            assets,
            promotion,
            crate::cas_projection::input_replay::point_limit(),
        )
    };
    match read() {
        Ok(status) => Ok(status),
        Err(_) => {
            validator
                .ensure_current()
                .map_err(|error| failure::from_coordinator(&error, validator.home_generation()))?;
            read().map_err(|error| {
                failure::from_input_admission_build(&error, validator.home_generation())
            })
        }
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn current_selected_path(
    home: &beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    thread_id: SyndicThreadId,
    home_generation: beryl_home_store::HomeGeneration,
) -> Result<SelectedPathProof, SchedulerFailure> {
    let thread = storage
        .thread(
            home,
            thread_id,
            crate::cas_projection::input_replay::point_limit(),
        )
        .map_err(|error| failure::from_syndic_read(&error, home_generation))?
        .ok_or(SchedulerFailure::Fatal)?;
    Ok(SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    ))
}

pub(in crate::cas_projection::accepted_input_scheduler) fn current_timestamp(
    floor: SyndicTimestamp,
) -> Result<SyndicTimestamp, ()> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ())?
        .as_millis();
    let millis = u64::try_from(millis).map_err(|_| ())?;
    Ok(SyndicTimestamp::from_unix_millis(
        millis.max(floor.unix_millis()),
    ))
}

pub(super) fn fresh_turn_id() -> Result<SyndicTurnId, ()> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    Ok(SyndicTurnId::from_bytes(bytes))
}

pub(super) fn fresh_item_id() -> Result<SyndicItemId, ()> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    Ok(SyndicItemId::from_bytes(bytes))
}
