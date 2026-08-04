use beryl_home_store::DomainReader;
use beryl_model::SyndicItemId;

use crate::mutation::required;
use crate::validation::{ProviderFrameStorageValidationError, validate_staged_provider_frame};
use crate::{
    ContentEncoding, ContentLifecycle, ContentManifestRecord, ProviderItemBuildLifecycle,
    ProviderItemBuildRecord, ProviderItemStreamValidatorV1, SealedProviderFrameReference,
    SourceEventRecord, SyndicMutationError, codec::*, content_chain_seed, domain::SyndicDomain,
};

use super::helpers::exact_item_source;

pub(super) fn validate_build_identity(
    event: &SourceEventRecord,
    item_id: SyndicItemId,
    frame: &SealedProviderFrameReference,
    build: &ProviderItemBuildRecord,
) -> Result<(), SyndicMutationError> {
    let source = exact_item_source(event, frame)?;
    if build.item_id() != item_id
        || build.turn_id() != event.turn_id()
        || build.source_event() != event.sequence()
        || build.source() != &source
        || build.target() != frame
        || build.lifecycle() != ProviderItemBuildLifecycle::Sealed
    {
        return Err(SyndicMutationError::ProviderFrameBuildConflict);
    }
    Ok(())
}

pub(super) fn validate_structural_frame(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderItemBuildRecord,
) -> Result<crate::ProviderFrameStructuralValidationV1, SyndicMutationError> {
    let structural = match validate_staged_provider_frame(reader, build) {
        Ok(value) => value,
        Err(ProviderFrameStorageValidationError::Read(source)) => return Err(source.into()),
        Err(ProviderFrameStorageValidationError::Invariant(_)) => {
            return Err(SyndicMutationError::ProviderFrameValidationConflict);
        }
    };
    let mut stream = build
        .prior()
        .map_or_else(ProviderItemStreamValidatorV1::new, |prior| {
            ProviderItemStreamValidatorV1::from_state(prior.stream_state().clone())
        });
    stream
        .observe_structural(&structural)
        .map_err(|_| SyndicMutationError::ProviderFrameValidationConflict)?;
    if structural.reference() != build.target().frame()
        || structural.observation() != build.target().observation()
        || stream.state() != Some(build.target().stream_state())
    {
        return Err(SyndicMutationError::ProviderFrameValidationConflict);
    }
    Ok(structural)
}

pub(super) fn publication_manifest(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderItemBuildRecord,
) -> Result<ContentManifestRecord, SyndicMutationError> {
    let target = build.target().content();
    let current = required::<ContentManifestsFamily>(reader, &target.id())?;
    let common = current.id() == target.id()
        && current.owner() == Some(build.item_id())
        && current.encoding() == ContentEncoding::ProviderItemV1;
    let selected_frontier = match build.prior() {
        Some(prior) => {
            current.lifecycle() == ContentLifecycle::Live
                && current.current_reference() == Some(prior.content())
        }
        None => {
            current.lifecycle() == ContentLifecycle::Building
                && current.revision() == target.revision()
                && current.chunk_count() == 0
                && current.encoded_bytes() == 0
                && current.chain_digest() == content_chain_seed(ContentEncoding::ProviderItemV1)
                && current.expected() == target.summary()
        }
    };
    if !common || !selected_frontier {
        return Err(SyndicMutationError::ContentManifestConflict);
    }
    Ok(ContentManifestRecord::with_owner(
        target.id(),
        Some(build.item_id()),
        target.revision(),
        ContentEncoding::ProviderItemV1,
        ContentLifecycle::Live,
        target.summary().chunk_count(),
        target.summary().encoded_bytes(),
        target.summary().digest(),
        target.summary(),
    ))
}
