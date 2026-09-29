use beryl_home_store::{HomeGeneration, HomeRecoveryCandidate, TurnStartAdmissionRequirement};
use beryl_model::BerylHomeId;
use beryl_state::AssetState;

use crate::{
    cas_projection::{NativeLineageRecoveryControl, ProjectionConnectionService},
    composer_marker_seal::DraftMarkerSealService,
    main_window::MainWindowComposerSubmissionRequestSource,
};

pub(crate) struct PreparedComposerRecoveryAdapters {
    home: BerylHomeId,
    generation: HomeGeneration,
    assets: AssetState,
    marker: DraftMarkerSealService,
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
    pub(super) fn from_prepared(
        candidate: &HomeRecoveryCandidate,
        assets: AssetState,
        cas: &ProjectionConnectionService,
        marker: &DraftMarkerSealService,
        requirement: TurnStartAdmissionRequirement,
    ) -> Self {
        Self {
            home: candidate.home_id(),
            generation: candidate.generation(),
            assets,
            marker: marker.clone(),
            submission: MainWindowComposerSubmissionRequestSource::new(
                cas.submission_execution_wake(),
                requirement,
            ),
            native: cas.native_lineage_recovery_control(),
        }
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
        (self.assets, self.marker, self.submission, self.native)
    }
}
