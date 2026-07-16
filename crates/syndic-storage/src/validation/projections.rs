use beryl_home_store::DomainReader;

use crate::{domain::SyndicDomain, error::SyndicValidationError};

mod events;
mod item_replay;
mod items;
mod membership;
mod records;
mod resources;
mod source;
mod transcript;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    events::validate(reader)?;
    items::validate_items(reader)?;
    items::validate_turn_items(reader)?;
    source::validate(reader)?;
    items::validate_cas_items(reader)?;
    records::validate(reader)?;
    membership::validate_stable(reader)?;
    membership::validate_generation_suffixes(reader)?;
    item_replay::validate(reader)?;
    membership::validate_heads(reader)?;
    resources::validate_metadata(reader)?;
    resources::validate_indexes(reader)?;
    transcript::validate(reader)
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
