use beryl_home_store::DomainReader;

use crate::{domain::SyndicDomain, error::SyndicValidationError};

mod bindings;
mod content;
mod graph;
mod ordering;
mod projections;
mod scan;

#[cfg(feature = "test-faults")]
pub(crate) use scan::{PAGE_BYTES as VALIDATION_PAGE_BYTES, PAGE_ITEMS as VALIDATION_PAGE_ITEMS};

pub(crate) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    content::validate(reader)?;
    graph::validate(reader)?;
    ordering::validate(reader)?;
    projections::validate(reader)?;
    graph::validate_context_envelopes(reader)?;
    bindings::validate(reader)
}
