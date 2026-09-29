use beryl_home_store::{HomeGeneration, HomeRecoveryCandidate, TurnStartAdmissionRequirement};
use beryl_model::BerylHomeId;
use beryl_state::AssetState;
use syndic_storage::SyndicStorage;

use crate::{
    cas_projection::{NativeLineageRecoveryControl, ProjectionConnectionService},
    composer_marker_seal::{
        DraftMarkerSealService, DraftMarkerSealServiceLimits,
        initial_preparation::PreparedMarkerServices,
    },
    main_window::MainWindowComposerSubmissionRequestSource,
};

pub(crate) struct PreparedComposerRecoveryAdapters {
    home: BerylHomeId,
    generation: HomeGeneration,
    assets: AssetState,
    marker: PreparedMarkerServices,
    submission: MainWindowComposerSubmissionRequestSource,
    native: NativeLineageRecoveryControl,
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../tests/unit/recovery_mount_attachment.rs"]
mod mount_attachment;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../tests/unit/recovery_composer_support.rs"]
pub(crate) mod test_support;

impl PreparedComposerRecoveryAdapters {
    pub(crate) fn prepare(
        candidate: &mut HomeRecoveryCandidate,
        storage: SyndicStorage,
        assets: AssetState,
        cas: &ProjectionConnectionService,
        limits: DraftMarkerSealServiceLimits,
        requirement: TurnStartAdmissionRequirement,
    ) -> Result<Self, String> {
        if cas.home_id() != candidate.home_id() || cas.home_generation() != candidate.generation() {
            return Err("composer recovery CAS service belongs to another candidate".into());
        }
        let marker =
            PreparedMarkerServices::prepare_recovery(candidate, storage, assets.clone(), limits)
                .map_err(|error| error.to_string())?;
        Ok(Self {
            home: candidate.home_id(),
            generation: candidate.generation(),
            assets,
            marker,
            submission: MainWindowComposerSubmissionRequestSource::new(
                cas.submission_execution_wake(),
                requirement,
            ),
            native: cas.native_lineage_recovery_control(),
        })
    }

    pub(crate) fn matches(&self, home: BerylHomeId, generation: HomeGeneration) -> bool {
        self.home == home && self.generation == generation
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        AssetState,
        DraftMarkerSealService,
        MainWindowComposerSubmissionRequestSource,
        NativeLineageRecoveryControl,
    ) {
        (
            self.assets,
            self.marker.into_service(),
            self.submission,
            self.native,
        )
    }
}
