use crate::{command::MaterializedDomainDescriptor, domain::DomainRegistry};

use super::{
    AssetExecution, FirstAcceptancePromotionAdmission, FirstAcceptancePromotionAssetReservation,
    FirstAcceptancePromotionObservation, FirstAcceptancePromotionPointFact,
    FirstAcceptancePromotionSourceReservation, correlation_digest,
};

pub(crate) enum FirstAcceptancePromotionReservation {
    Source(FirstAcceptancePromotionSourceReservation),
    Asset(FirstAcceptancePromotionAssetReservation),
}

pub(crate) struct FirstAcceptancePromotionDescriptor {
    pub(crate) source_slot: usize,
    pub(crate) source: FirstAcceptancePromotionSourceReservation,
    pub(crate) asset_slot: Option<usize>,
    pub(crate) asset: Option<FirstAcceptancePromotionAssetReservation>,
}

pub(crate) struct FirstAcceptancePromotionExecution {
    pub(crate) resolved: bool,
    pub(crate) diagnostic: Option<[u8; 32]>,
    pub(crate) source: FirstAcceptancePromotionSourceResult,
    pub(crate) asset: Option<FirstAcceptancePromotionAssetResult>,
    pub(crate) points: [Option<FirstAcceptancePromotionPointFact>; 3],
}

#[derive(Clone, Copy)]
pub(crate) enum FirstAcceptancePromotionSourceResult {
    Missing,
    Authenticated,
    Unresolved,
    Collision,
}

#[derive(Clone, Copy)]
pub(crate) enum FirstAcceptancePromotionAssetResult {
    Exact,
    Collision,
}

impl FirstAcceptancePromotionDescriptor {
    pub(crate) fn unrun_collision(&self) -> FirstAcceptancePromotionExecution {
        FirstAcceptancePromotionExecution {
            resolved: false,
            diagnostic: None,
            source: FirstAcceptancePromotionSourceResult::Missing,
            asset: None,
            points: [None; 3],
        }
    }

    fn collision_after_authentication(
        diagnostic: [u8; 32],
        asset: Option<FirstAcceptancePromotionAssetResult>,
        points: [Option<FirstAcceptancePromotionPointFact>; 3],
    ) -> FirstAcceptancePromotionExecution {
        FirstAcceptancePromotionExecution {
            resolved: false,
            diagnostic: Some(diagnostic),
            source: FirstAcceptancePromotionSourceResult::Authenticated,
            asset,
            points,
        }
    }

    pub(crate) fn execute(
        &self,
        snapshot: &fjall::Snapshot,
        registry: &DomainRegistry,
        domains: &[MaterializedDomainDescriptor],
    ) -> Result<
        FirstAcceptancePromotionExecution,
        (&'static str, crate::domain::callback::ErasedCallbackError),
    > {
        let Some(source_domain) = registry.get(self.source_slot) else {
            return Ok(self.unrun_collision());
        };
        if source_domain.name != self.source.domain || source_domain.owner != self.source.owner {
            return Ok(self.unrun_collision());
        }
        let Some(source_descriptor) = domains
            .iter()
            .find(|domain| domain.domain_slot == self.source_slot)
        else {
            return Ok(self.unrun_collision());
        };
        let observation = (self.source.authenticate)(snapshot, source_domain, source_descriptor)
            .map_err(|error| (self.source.domain, error))?;
        let correlation = match observation {
            FirstAcceptancePromotionObservation::Authenticated(correlation) => correlation,
            FirstAcceptancePromotionObservation::Unresolved => {
                let mut execution = self.unrun_collision();
                execution.source = FirstAcceptancePromotionSourceResult::Unresolved;
                return Ok(execution);
            }
            FirstAcceptancePromotionObservation::Collision => {
                let mut execution = self.unrun_collision();
                execution.source = FirstAcceptancePromotionSourceResult::Collision;
                return Ok(execution);
            }
        };
        let diagnostic = correlation_digest(&correlation);
        match (self.source.admission, self.asset.as_ref(), self.asset_slot) {
            (FirstAcceptancePromotionAdmission::MarkerFree, None, None)
                if correlation.asset_reference_set().is_none() =>
            {
                Ok(FirstAcceptancePromotionExecution {
                    resolved: true,
                    diagnostic: Some(diagnostic),
                    source: FirstAcceptancePromotionSourceResult::Authenticated,
                    asset: None,
                    points: [None; 3],
                })
            }
            (
                FirstAcceptancePromotionAdmission::AssetTransferRequired,
                Some(asset),
                Some(asset_slot),
            ) if asset.seed.draft_id.accepted_input_id() == asset.seed.accepted_input_id
                && correlation.accepted_input_id() == asset.seed.accepted_input_id
                && correlation.asset_reference_set() == Some(asset.seed.asset_reference_set) =>
            {
                let Some(asset_domain) = registry.get(asset_slot) else {
                    return Ok(Self::collision_after_authentication(
                        diagnostic,
                        Some(FirstAcceptancePromotionAssetResult::Collision),
                        [None; 3],
                    ));
                };
                if asset_domain.name != asset.domain
                    || asset_domain.owner != asset.owner
                    || !asset_domain
                        .families
                        .iter()
                        .any(|family| family.codec_type == asset.codec_type)
                {
                    return Ok(Self::collision_after_authentication(
                        diagnostic,
                        Some(FirstAcceptancePromotionAssetResult::Collision),
                        [None; 3],
                    ));
                }
                match (asset.execute)(snapshot, asset_domain, asset.seed, correlation)
                    .map_err(|error| (asset.domain, error))?
                {
                    AssetExecution::Exact { points } => Ok(FirstAcceptancePromotionExecution {
                        resolved: true,
                        diagnostic: Some(diagnostic),
                        source: FirstAcceptancePromotionSourceResult::Authenticated,
                        asset: Some(FirstAcceptancePromotionAssetResult::Exact),
                        points,
                    }),
                    AssetExecution::Collision { points } => {
                        Ok(Self::collision_after_authentication(
                            diagnostic,
                            Some(FirstAcceptancePromotionAssetResult::Collision),
                            points,
                        ))
                    }
                }
            }
            _ => Ok(Self::collision_after_authentication(
                diagnostic, None, [None; 3],
            )),
        }
    }
}
