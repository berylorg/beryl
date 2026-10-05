use beryl_home_store::{HomeGeneration, HomeRecoveryCandidate, TurnStartAdmissionRequirement};
use beryl_model::BerylHomeId;
use beryl_state::AssetState;

use crate::{
    cas_projection::{NativeLineageRecoveryControl, ProjectionConnectionService},
    composer_marker_seal::DraftMarkerSealService,
    main_window::MainWindowComposerSubmissionRequestSource,
};

pub(crate) struct PreparedComposerRecoveryAdapters {
    private_clipboard: Option<crate::main_window::MainWindowPrivateClipboardOwner>,
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
            private_clipboard: None,
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

    pub(super) fn with_private_clipboard_owner(
        mut self,
        owner: crate::main_window::MainWindowPrivateClipboardOwner,
    ) -> Self {
        self.private_clipboard = Some(owner);
        self
    }

    pub(crate) fn private_clipboard_owner(
        &self,
    ) -> Option<crate::main_window::MainWindowPrivateClipboardOwner> {
        self.private_clipboard.clone()
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
