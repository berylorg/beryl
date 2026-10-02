use super::*;

pub struct DraftEditorCandidatePublicationCorrespondenceV1 {
    home_id: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    candidate: DraftEditorCandidateActivationBindingV1,
    selector: DraftEditorCurrentSelectorV1,
}

impl DraftEditorCandidatePublicationCorrespondenceV1 {
    pub const fn home_id(&self) -> beryl_model::BerylHomeId {
        self.home_id
    }
    pub const fn generation(&self) -> beryl_home_store::HomeGeneration {
        self.generation
    }
    pub const fn candidate(&self) -> DraftEditorCandidateActivationBindingV1 {
        self.candidate
    }
    pub const fn selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }
}

pub struct DraftEditorCandidateSavedCorrespondenceV1 {
    home_id: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    candidate: DraftEditorCandidateActivationBindingV1,
    selector: DraftEditorCurrentSelectorV1,
}

impl DraftEditorCandidateSavedCorrespondenceV1 {
    pub const fn home_id(&self) -> beryl_model::BerylHomeId {
        self.home_id
    }
    pub const fn generation(&self) -> beryl_home_store::HomeGeneration {
        self.generation
    }
    pub const fn candidate(&self) -> DraftEditorCandidateActivationBindingV1 {
        self.candidate
    }
    pub const fn selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }
}

impl SyndicStorage {
    pub fn qualify_retained_draft_editor_candidate_after_publication_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        retained: DraftEditorCandidateActivationBindingV1,
        prepared: &PreparedDraftEditorCandidatePublicationV1,
    ) -> Result<
        DraftEditorCandidatePublicationCorrespondenceV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let access = ReadAccess::Candidate(store);
        let revision = self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?;
        let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(record)) =
            self.point_with_access::<DraftEditorCandidateSessionsFamily>(
                access,
                publication_key(prepared.request),
                point_limit(),
            )?
        else {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted);
        };
        let receipt = record
            .publication()
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if receipt.request_bytes() != prepared.canonical_request
            || receipt.captured_frontier() != &prepared.captured_frontier
            || !validate_publication_receipt_with_access(self, access, receipt)?
            || DraftEditorCandidateActivationBindingV1::from_head(receipt.before_head()) != retained
            || prepared.request.candidate_generation() >= retained.candidate_generation()
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let DraftEditorCandidateSessionReadOutcomeV1::Active(head) =
            publication_session_with_access(
                self,
                access,
                retained.draft_id(),
                retained.session_id(),
            )?
        else {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        };
        if &head != receipt.after_head()
            || head.newest_candidate_generation() != retained.candidate_generation()
            || head.newest_root() != retained.root()
            || head.newest_history() != retained.history()
            || head.logical_extent() != retained.logical_extent()
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let candidate = DraftEditorCandidateActivationBindingV1::from_head(&head);
        let selector = receipt.successor_selector();
        if self.draft_editor_candidate_is_saved_candidate(store, candidate, selector)? {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        if self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "retained candidate publication correspondence",
            }
            .into());
        }
        Ok(DraftEditorCandidatePublicationCorrespondenceV1 {
            home_id: store.home_id(),
            generation: store.generation(),
            candidate,
            selector,
        })
    }

    pub fn qualify_saved_draft_editor_candidate_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        candidate: DraftEditorCandidateActivationBindingV1,
        selector: DraftEditorCurrentSelectorV1,
    ) -> Result<
        DraftEditorCandidateSavedCorrespondenceV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        if !self.draft_editor_candidate_is_saved_candidate(store, candidate, selector)? {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted);
        }
        Ok(DraftEditorCandidateSavedCorrespondenceV1 {
            home_id: store.home_id(),
            generation: store.generation(),
            candidate,
            selector,
        })
    }

    pub fn qualify_published_draft_editor_candidate_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        retained: DraftEditorCandidateActivationBindingV1,
        prepared: &PreparedDraftEditorCandidatePublicationV1,
    ) -> Result<
        DraftEditorCandidateSavedCorrespondenceV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let access = ReadAccess::Candidate(store);
        let revision = self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?;
        if DraftEditorCandidateActivationBindingV1::from_head(&prepared.captured_head) != retained {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(record)) =
            self.point_with_access::<DraftEditorCandidateSessionsFamily>(
                access,
                publication_key(prepared.request),
                point_limit(),
            )?
        else {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted);
        };
        let receipt = record
            .publication()
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if receipt.request_bytes() != prepared.canonical_request
            || receipt.captured_frontier() != &prepared.captured_frontier
            || !validate_publication_receipt_with_access(self, access, receipt)?
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let DraftEditorCandidateSessionReadOutcomeV1::Active(head) =
            publication_session_with_access(
                self,
                access,
                retained.draft_id(),
                retained.session_id(),
            )?
        else {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        };
        if &head != receipt.after_head()
            || head.newest_candidate_generation() != retained.candidate_generation()
            || head.newest_root() != retained.root()
            || head.newest_history() != prepared.captured_frontier.reference()
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let qualified = self.qualify_saved_draft_editor_candidate_candidate(
            store,
            DraftEditorCandidateActivationBindingV1::from_head(&head),
            receipt.successor_selector(),
        )?;
        if self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "published candidate saved correspondence",
            }
            .into());
        }
        Ok(qualified)
    }
}
