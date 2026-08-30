use beryl_home_store::DomainReader;

use crate::{domain::SyndicDomain, error::SyndicValidationError};

mod bindings;
mod compaction;
mod content;
mod draft_marker_admission;
pub(crate) use content::{read_encoded_range, read_projection_text_range};
mod graph;
mod ordering;
mod projections;
mod provider_frame;
mod provider_observation;
mod queries;
mod scan;
mod stop;
mod thread_properties;

pub(crate) use provider_frame::{
    ProviderFrameStorageValidationError, advance_provider_completion_comparison,
    validate_staged_provider_frame,
};

#[cfg(feature = "test-faults")]
pub(crate) use scan::{PAGE_BYTES as VALIDATION_PAGE_BYTES, PAGE_ITEMS as VALIDATION_PAGE_ITEMS};

pub(crate) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    content::validate(reader)?;
    draft_marker_admission::validate(reader)?;
    compaction::validate(reader)?;
    graph::validate(reader)?;
    ordering::validate(reader)?;
    queries::validate(reader)?;
    projections::validate(reader)?;
    provider_observation::validate(reader)?;
    thread_properties::validate(reader)?;
    graph::validate_context_envelopes(reader)?;
    bindings::validate(reader)?;
    stop::validate(reader)
}
