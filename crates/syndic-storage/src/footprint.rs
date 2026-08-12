use beryl_home_store::{
    CheckedBatchFootprint, DurableStartFootprintError, RecordCodec, SyndicDurableStartFootprint,
    participating_domain_footprint,
};

use crate::{codec::*, domain::SyndicDomain};

/// Returns the owner-derived maximum footprint for an idle draft submission.
///
/// The maximum includes every optional V5 write set: child and parent indexes,
/// image-label origin span, transcript build, and the two-record context move.
pub fn idle_submission_max_footprint()
-> Result<SyndicDurableStartFootprint, DurableStartFootprintError> {
    SyndicDurableStartFootprint::idle_submission(
        idle_submission_mutation_footprint()?,
        participating_domain_footprint::<SyndicDomain>()?,
    )
}

/// Returns the owner-derived maximum footprint for accepted-input promotion.
///
/// The maximum includes the optional route head, transcript build, and parent
/// index, and uses the larger next-source replacement shape rather than its
/// deletion branch.
pub fn accepted_input_promotion_max_footprint()
-> Result<SyndicDurableStartFootprint, DurableStartFootprintError> {
    SyndicDurableStartFootprint::accepted_input_promotion(
        accepted_input_promotion_mutation_footprint()?,
        participating_domain_footprint::<SyndicDomain>()?,
    )
}

fn idle_submission_mutation_footprint() -> Result<CheckedBatchFootprint, DurableStartFootprintError>
{
    let mut footprint = delete::<DraftsCodec>()?;
    for record in [
        put::<ThreadsCodec>()?,
        put::<DraftsCodec>()?,
        put::<DraftByThreadCodec>()?,
        put::<TurnsCodec>()?,
        put::<TurnStatesCodec>()?,
        put::<TurnChildrenCodec>()?,
        put::<CanonicalItemsCodec>()?,
        put::<TurnItemsCodec>()?,
        put::<ImageLabelOriginSpansCodec>()?,
        put::<TranscriptHeadsCodec>()?,
        put::<TranscriptBuildsCodec>()?,
        put::<HistorySummariesCodec>()?,
        put::<InputGatesCodec>()?,
        put::<ActivityQueryHeadsCodec>()?,
        put::<ActivityQuerySourcesCodec>()?,
        put::<BindingsCodec>()?,
        put::<BindingHeadsCodec>()?,
        delete::<ContextEnvelopesCodec>()?,
        put::<ContextEnvelopesCodec>()?,
        put::<ThreadParentCodec>()?,
    ] {
        footprint = footprint.checked_add(record)?;
    }
    Ok(footprint)
}

fn accepted_input_promotion_mutation_footprint()
-> Result<CheckedBatchFootprint, DurableStartFootprintError> {
    let mut footprint = put::<AcceptedRouteGenerationHeadsCodec>()?;
    for record in [
        put::<AcceptedRouteGenerationsCodec>()?,
        put::<AcceptedRouteLeavesCodec>()?,
        put::<AcceptedNextSourcesCodec>()?,
        put::<ThreadsCodec>()?,
        put::<DraftByThreadCodec>()?,
        put::<TurnsCodec>()?,
        put::<TurnStatesCodec>()?,
        put::<TurnChildrenCodec>()?,
        put::<CanonicalItemsCodec>()?,
        put::<TurnItemsCodec>()?,
        put::<TranscriptHeadsCodec>()?,
        put::<TranscriptBuildsCodec>()?,
        put::<HistorySummariesCodec>()?,
        put::<InputGatesCodec>()?,
        put::<ActivityQueryHeadsCodec>()?,
        put::<ActivityQuerySourcesCodec>()?,
        put::<BindingsCodec>()?,
        put::<BindingHeadsCodec>()?,
        put::<ThreadParentCodec>()?,
    ] {
        footprint = footprint.checked_add(record)?;
    }
    Ok(footprint)
}

fn put<R: RecordCodec<SyndicDomain>>() -> Result<CheckedBatchFootprint, DurableStartFootprintError>
{
    Ok(CheckedBatchFootprint::new(
        1,
        usize_to_u64(R::MAX_KEY_BYTES)?,
        usize_to_u64(
            R::MAX_VALUE_BYTES
                .checked_add(beryl_home_store::RECORD_VERSION_BYTES)
                .ok_or(DurableStartFootprintError::ArithmeticOverflow)?,
        )?,
    ))
}

fn delete<R: RecordCodec<SyndicDomain>>()
-> Result<CheckedBatchFootprint, DurableStartFootprintError> {
    Ok(CheckedBatchFootprint::new(
        1,
        usize_to_u64(R::MAX_KEY_BYTES)?,
        0,
    ))
}

fn usize_to_u64(value: usize) -> Result<u64, DurableStartFootprintError> {
    u64::try_from(value).map_err(|_| DurableStartFootprintError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_submission_max_includes_every_optional_write_branch() {
        let footprint = idle_submission_mutation_footprint().expect("checked footprint");
        assert_eq!(21, footprint.records());
        assert_eq!(442, footprint.encoded_key_bytes());
        assert_eq!(1_245_772, footprint.encoded_value_bytes());
    }

    #[test]
    fn accepted_input_promotion_max_includes_every_optional_write_branch() {
        let footprint = accepted_input_promotion_mutation_footprint().expect("checked footprint");
        assert_eq!(20, footprint.records());
        assert_eq!(432, footprint.encoded_key_bytes());
        assert_eq!(1_310_800, footprint.encoded_value_bytes());
    }
}
