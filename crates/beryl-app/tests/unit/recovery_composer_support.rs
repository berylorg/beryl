use super::*;
use crate::composer_marker_seal::{
    DraftMarkerSealServiceLimits, initial_preparation::PreparedMarkerServices,
};
use std::num::NonZeroUsize;
use syndic_storage::SyndicStorage;

pub(crate) fn adapters(candidate: &mut HomeRecoveryCandidate) -> PreparedComposerRecoveryAdapters {
    let assets = beryl_state::BerylState::reacquire_candidate(candidate)
        .unwrap()
        .assets();
    let storage = SyndicStorage::reacquire_candidate(candidate).unwrap();
    let marker = PreparedMarkerServices::prepare_recovery(
        candidate,
        storage,
        assets.clone(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(2).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    PreparedComposerRecoveryAdapters {
        home: candidate.home_id(),
        generation: candidate.generation(),
        assets,
        marker: marker.into_service(),
        submission: MainWindowComposerSubmissionRequestSource::new(
            crate::cas_projection::SubmissionExecutionWake::storage_only_for_test(),
            crate::cas_projection::ProjectionServiceConfig::try_new(
                1,
                4,
                beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
            )
            .unwrap()
            .turn_start_admission_requirement(),
        ),
        native: NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap()),
    }
}
