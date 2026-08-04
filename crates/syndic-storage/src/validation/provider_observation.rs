use beryl_home_store::DomainReader;

use crate::{
    ProviderObservationBuildLifecycle, ProviderObservationBuildRecord,
    codec::*,
    domain::SyndicDomain,
    error::SyndicValidationError,
    provider_observation::{
        CanonicalObservationState, ProviderObservationValidatorState, replay_chunk,
    },
};

use super::scan::{point, scan};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ProviderObservationBuildsFamily>(reader, |key, build| {
        if *key != build.identity() {
            return Err(SyndicValidationError::Invariant(
                "provider-observation build key/value mismatch",
            ));
        }
        validate_build(reader, build)
    })?;
    scan::<ProviderObservationChunksFamily>(reader, |key, chunk| {
        if key.identity() != chunk.identity() || key.ordinal() != chunk.ordinal() {
            return Err(SyndicValidationError::Invariant(
                "provider-observation chunk key/value mismatch",
            ));
        }
        let Some(build) = point::<ProviderObservationBuildsFamily>(reader, &key.identity())? else {
            return Err(SyndicValidationError::Invariant(
                "provider-observation chunk has no owning build",
            ));
        };
        if key.ordinal() == 0 || key.ordinal() > build.chunk_count() {
            return Err(SyndicValidationError::Invariant(
                "provider-observation chunk is outside its durable frontier",
            ));
        }
        Ok(())
    })
}

fn validate_build(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderObservationBuildRecord,
) -> Result<(), SyndicValidationError> {
    let mut validator = ProviderObservationValidatorState::initial();
    let mut canonical = CanonicalObservationState::initial(build.begin());
    for ordinal in 1..=build.chunk_count() {
        let key = ProviderObservationChunkKey::new(build.identity(), ordinal);
        let Some(chunk) = point::<ProviderObservationChunksFamily>(reader, &key)? else {
            return Err(SyndicValidationError::Invariant(
                "provider-observation build frontier has a missing chunk",
            ));
        };
        if chunk.identity() != build.identity() || chunk.ordinal() != ordinal {
            return Err(SyndicValidationError::Invariant(
                "provider-observation build frontier has a mismatched chunk",
            ));
        }
        replay_chunk(build.begin(), &mut validator, &mut canonical, &chunk).map_err(|_| {
            SyndicValidationError::Invariant(
                "provider-observation chunk replay is structurally invalid",
            )
        })?;
    }
    if build.validator() != &validator
        || build.canonical_bytes() != canonical.canonical_bytes()
        || build.digest() != canonical.digest()
    {
        return Err(SyndicValidationError::Invariant(
            "provider-observation durable frontier disagrees with chunk replay",
        ));
    }
    if build.lifecycle() == ProviderObservationBuildLifecycle::Sealed {
        validator.finish(build.begin()).map_err(|_| {
            SyndicValidationError::Invariant(
                "sealed provider observation is not structurally complete",
            )
        })?;
    }
    Ok(())
}
