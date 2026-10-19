use super::super::import::read::{SourceReader, StoreSourceReader};
use super::super::*;
use super::*;
use crate::{SyndicStorage, codec::*, domain::SyndicDomain};
use beryl_home_store::{DomainReader, HomeStore};

pub(crate) fn required<F: Family>(
    r: &impl SourceReader,
    key: &F::Key,
) -> Result<F::Value, DraftComposerRecallErrorV1> {
    r.point::<F>(key)?
        .ok_or(DraftComposerRecallErrorV1::Conflict)
}

pub(crate) fn authenticate_basis(
    r: &impl SourceReader,
    basis: &DraftComposerRecallBasisV1,
) -> Result<AcceptanceDisposalV1, DraftComposerRecallErrorV1> {
    authenticated_recall_basis(&SourceAuthorityReader(r), basis)?
        .ok_or(DraftComposerRecallErrorV1::Proof)
}

struct SourceAuthorityReader<'a, R>(&'a R);
impl<R: SourceReader> RecallAuthorityReader for SourceAuthorityReader<'_, R> {
    type Error = crate::SyndicReadError;
    fn recall_point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, Self::Error> {
        self.0.point::<F>(key)
    }
}

pub(crate) struct AccessRecallAuthorityReader<'a> {
    pub storage: &'a SyndicStorage,
    pub access: crate::read::access::ReadAccess<'a>,
}
impl RecallAuthorityReader for AccessRecallAuthorityReader<'_> {
    type Error = crate::SyndicReadError;
    fn recall_point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, Self::Error> {
        self.storage
            .point_with_access::<F>(self.access, key.clone(), point_limit())
    }
}

impl SyndicStorage {
    pub fn authenticate_composer_accepted_input(
        &self,
        store: &HomeStore,
        source: &ComposerAcceptedInputReferenceV1,
    ) -> Result<(), DraftComposerRecallErrorV1> {
        self.composer_recall_basis(store, source).map(|_| ())
    }
    pub(crate) fn composer_recall_basis(
        &self,
        store: &HomeStore,
        source: &ComposerAcceptedInputReferenceV1,
    ) -> Result<DraftComposerRecallBasisV1, DraftComposerRecallErrorV1> {
        if store.home_id() != source.home_id() {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        let revision = self.revision(store)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let receipt = required::<DraftEditorCandidateSessionsFamily>(&r, &source.key())?;
        let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = receipt else {
            return Err(DraftComposerRecallErrorV1::Proof);
        };
        let disposal = receipt
            .disposal()
            .ok_or(DraftComposerRecallErrorV1::Proof)?;
        if disposal.acceptance().map(|a| &a.request) != Some(source.acceptance.as_ref()) {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        let basis = DraftComposerRecallBasisV1 {
            key: source.key(),
            acceptance_digest: acceptance_disposal_digest(disposal)
                .map_err(|_| DraftComposerRecallErrorV1::Proof)?,
            thread: source.thread_id(),
            content: source.content(),
            assets: source.asset_reference_set(),
        };
        authenticate_basis(&r, &basis)?;
        if self.revision(store)? != revision {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(basis)
    }
    pub fn capture_composer_browse_checkpoint(
        &self,
        store: &HomeStore,
        activation: DraftEditorCandidateActivationBindingV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
    ) -> Result<DraftComposerBrowseCheckpointV1, DraftComposerRecallErrorV1> {
        let revision = self.revision(store)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let head = r.session_head(&DraftEditorCandidateSessionRecordKeyV1::head(
            activation.draft_id(),
            activation.session_id(),
        ))?;
        if DraftEditorCandidateActivationBindingV1::from_head(&head) != activation
            || !session::idle_candidate_closure_is_exact_with_access(
                self,
                crate::read::access::ReadAccess::Ordinary(store),
                &head,
            )?
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        require_destination(&r, &head)?;
        validate_position(self, store, activation.root(), caret)?;
        validate_position(self, store, activation.root(), selection)?;
        if self.revision(store)? != revision {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(DraftComposerBrowseCheckpointV1 {
            home: store.home_id(),
            activation,
            caret,
            selection,
        })
    }
}

pub(crate) fn require_destination(
    r: &impl SourceReader,
    head: &DraftEditorCandidateSessionV1,
) -> Result<(), DraftComposerRecallErrorV1> {
    let thread = required::<ThreadsFamily>(r, &head.thread_id())?;
    let draft = required::<DraftsFamily>(r, &head.draft_id())?;
    if thread.current_draft_id() != head.draft_id()
        || draft.thread_id() != head.thread_id()
        || draft.submission_intent() != crate::DraftSubmissionIntent::Ordinary
        || head.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
    {
        return Err(DraftComposerRecallErrorV1::Ineligible);
    }
    Ok(())
}

pub(crate) fn require_writer_destination(
    r: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<(), DraftComposerRecallErrorV1> {
    require_destination(r, head)?;
    crate::mutation::discussion_mutation::require_editable(r, head.thread_id())?;
    Ok(())
}

pub(crate) fn source_owner(
    r: &impl SourceReader,
    basis: &DraftComposerRecallBasisV1,
) -> Result<beryl_model::ComposerRecallSourceOwnerV1, DraftComposerRecallErrorV1> {
    let acceptance = authenticate_basis(r, basis)?;
    match acceptance.result {
        AcceptanceResultV1::Idle { turn, item } => Ok(
            beryl_model::ComposerRecallSourceOwnerV1::SubmittedTurnItem {
                turn_id: turn,
                item_id: item,
            },
        ),
        AcceptanceResultV1::Accepted {
            input,
            ordinal,
            generation,
        } => {
            let record = required::<AcceptedInputsFamily>(r, &input)?;
            let leaf = required::<AcceptedRouteLeavesFamily>(r, &input)?;
            if record.id() != input
                || record.content() != basis.content
                || record.asset_reference_set() != basis.assets
                || leaf.input_id() != input
                || leaf.ordinal() != ordinal
                || leaf.generation() != generation
            {
                return Err(DraftComposerRecallErrorV1::Proof);
            }
            match leaf.promotion() {
                Some(promotion) => {
                    let item = required::<CanonicalItemsFamily>(r, &promotion.successor_item_id())?;
                    if item.presentation_content() != Some(record.content())
                        || item.presentation().asset_reference_set() != record.asset_reference_set()
                    {
                        return Err(DraftComposerRecallErrorV1::Proof);
                    }
                    Ok(
                        beryl_model::ComposerRecallSourceOwnerV1::SubmittedTurnItem {
                            turn_id: item.turn_id(),
                            item_id: item.id(),
                        },
                    )
                }
                None => Ok(beryl_model::ComposerRecallSourceOwnerV1::AcceptedInput(
                    input,
                )),
            }
        }
    }
}
