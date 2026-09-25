use beryl_home_store::{HomeCandidateError, HomeOpenPublication, ReadError};

use super::*;

pub(crate) struct PreparedMarkerServices {
    service: DraftMarkerSealService,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum MarkerPreparationError {
    #[error("marker service candidate access failed: {0}")]
    Candidate(#[from] HomeCandidateError),
    #[error("marker service Syndic authority validation failed: {0}")]
    Syndic(ReadError),
    #[error("marker service asset authority validation failed: {0}")]
    Asset(ReadError),
}

impl PreparedMarkerServices {
    pub(crate) fn prepare(
        candidate: &mut HomeOpenPublication,
        storage: SyndicStorage,
        assets: AssetState,
        limits: DraftMarkerSealServiceLimits,
    ) -> Result<Self, MarkerPreparationError> {
        let access = candidate.recovery_access()?;
        storage
            .revision_candidate(&access)
            .map_err(MarkerPreparationError::Syndic)?;
        assets
            .revision_candidate(&access)
            .map_err(MarkerPreparationError::Asset)?;
        let home_id = access.home_id();
        let service = DraftMarkerSealService {
            inner: new_shared_home_state(home_id, access.generation(), storage, assets, limits),
            home_id,
        };
        Ok(Self { service })
    }
}

impl Drop for PreparedMarkerServices {
    fn drop(&mut self) {
        self.service.retire_home_generation();
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/initial_marker_preparation.rs"
    ));
}
