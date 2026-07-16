use beryl_home_store::DomainReader;

use crate::{domain::SyndicDomain, error::SyndicValidationError};

use super::invariant;

mod builds;
mod cursor;
mod entries;
mod heads;
mod paths;
mod snapshot;
mod summaries;
mod visibility;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    heads::validate(reader)?;
    builds::validate(reader)?;
    paths::validate(reader)?;
    entries::validate(reader)?;
    summaries::validate(reader)
}
