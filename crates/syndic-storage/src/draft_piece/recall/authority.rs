use super::super::*;
use super::{AcceptanceDisposalV1, DraftComposerRecallBasisV1, acceptance_disposal_digest};
use crate::{ContentLifecycle, codec::*};

pub(crate) trait RecallAuthorityReader {
    type Error;
    fn recall_point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, Self::Error>;
}

pub(crate) fn authenticated_recall_basis<R: RecallAuthorityReader>(
    reader: &R,
    basis: &DraftComposerRecallBasisV1,
) -> Result<Option<AcceptanceDisposalV1>, R::Error> {
    use super::super::import::model::SealedComposerBasisV1;
    if !basis.is_locally_exact() {
        return Ok(None);
    }
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)) =
        reader.recall_point::<DraftEditorCandidateSessionsFamily>(&basis.key)?
    else {
        return Ok(None);
    };
    let Some(disposal) = receipt.disposal() else {
        return Ok(None);
    };
    let Some(acceptance) = disposal.acceptance() else {
        return Ok(None);
    };
    let a = &acceptance.request;
    if !acceptance.is_exact(disposal)
        || basis.key
            != DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
                a.draft_id(),
                a.candidate().session_id(),
                a.session_disposal_operation_id(),
            )
        || acceptance_disposal_digest(disposal).ok() != Some(basis.acceptance_digest)
        || basis.thread != a.thread_id()
        || basis.content != a.materialization().content()
        || basis.assets != a.asset_reference_set()
        || reader.recall_point::<DraftEditorCandidateSessionsFamily>(
            &DraftEditorCandidateSessionRecordKeyV1::head(a.draft_id(), a.candidate().session_id()),
        )? != Some(DraftEditorCandidateSessionRecordV1::Head(
            disposal.after_head().clone(),
        ))
        || reader.recall_point::<DraftComposerMaterializationsFamily>(&a.materialization().key())?
            != Some(a.materialization())
        || reader
            .recall_point::<DraftPieceRootsFamily>(&a.candidate().root().key())?
            .map(|root| root.reference())
            != Some(a.candidate().root())
        || reader
            .recall_point::<DraftEditHistoryFrontiersFamily>(&a.candidate().history().key())?
            .as_ref()
            != Some(disposal.frontier())
    {
        return Ok(None);
    }
    let content = basis.content;
    let Some(manifest) = reader.recall_point::<ContentManifestsFamily>(&content.id())? else {
        return Ok(None);
    };
    if manifest.id() != content.id()
        || manifest.revision() != content.revision()
        || manifest.encoding() != content.encoding()
        || manifest.lifecycle() != ContentLifecycle::Sealed
        || manifest.expected() != content.summary()
    {
        return Ok(None);
    }
    Ok(Some(acceptance.clone()))
}
