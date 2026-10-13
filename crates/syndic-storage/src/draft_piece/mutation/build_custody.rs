use super::*;

#[inline(never)]
pub(super) fn required_record<F: crate::codec::Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Box<F::Value>, SyndicMutationError> {
    Ok(Box::new(required::<F>(reader, key)?))
}

#[inline(never)]
pub(super) fn required_authenticated_build(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &DraftPieceSettlementKeyV1,
) -> Result<Box<DraftPieceBuildRecordV1>, SyndicMutationError> {
    Ok(Box::new(required_build(reader, key)?))
}

#[inline(never)]
pub(super) fn optional_authenticated_build(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &DraftPieceSettlementKeyV1,
) -> Result<Option<Box<DraftPieceBuildRecordV1>>, SyndicMutationError> {
    Ok(point_build(reader, key)?.map(Box::new))
}

#[inline(never)]
pub(super) fn acquired_build(
    acquisition: &advance_budget::BuildAcquisition<'_>,
    key: DraftPieceSettlementKeyV1,
) -> Result<Box<DraftPieceBuildRecordV1>, DraftPiecePrepareErrorV1> {
    Ok(Box::new(
        acquisition
            .point::<DraftPieceBuildsFamily>(key)?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
    ))
}

#[inline(never)]
pub(super) fn stored_build(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftPieceSettlementKeyV1,
) -> Result<Option<Box<DraftPieceBuildRecordV1>>, DraftPiecePrepareErrorV1> {
    Ok(storage
        .point::<DraftPieceBuildsFamily>(store, key, point_limit())?
        .map(Box::new))
}

#[inline(never)]
pub(super) fn stored_progress(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftPieceBuildProgressReceiptKeyV1,
) -> Result<Box<DraftPieceBuildProgressReceiptV1>, DraftPiecePrepareErrorV1> {
    Ok(Box::new(
        storage
            .point::<DraftPieceBuildProgressFamily>(store, key, point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
    ))
}
