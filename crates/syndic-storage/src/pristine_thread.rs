use beryl_home_store::{
    DomainMutation, DomainReader, DomainValidator, HomeStore, MutationBuilder,
    MutationContribution, ValidationContribution,
};
use beryl_model::{
    BindingRevision, DomainRevision, ExecutionBinding, SyndicDraftId, SyndicThreadId,
};

use crate::{
    ActivityQueryHeadRecord, BindingHeadRecord, BindingRecord, CreateThread, DraftByThreadRecord,
    DraftEditHistoryFrontierV1, DraftEditHistoryPolicyV1, DraftImageLabelProtectionHeadV1,
    DraftPieceRootRecordV1, DraftRecord, DraftSubmissionIntent, HistorySummaryRecord,
    ImageLabelAuthorityHeadV1, InputGateRecord, InputGateState, SelectedPathProof,
    SyndicMutationError, SyndicReadError, SyndicStorage, SyndicTimestamp, ThreadArchiveState,
    ThreadAttributesRecord, ThreadCatalogSummaryRecord, ThreadExecutionRecord, ThreadLineageDepth,
    ThreadRecord, ThreadUsageRecord, TranscriptBuildRecord, TranscriptGeneration,
    TranscriptViewHeadRecord,
    codec::{
        ActivityQueryHeadsCodec, ActivityQueryHeadsFamily, BindingHeadsCodec, BindingHeadsFamily,
        BindingKey, BindingsCodec, BindingsFamily, DraftByThreadCodec, DraftByThreadFamily,
        DraftImageLabelProtectionHeadsCodec, DraftImageLabelProtectionHeadsFamily, DraftsCodec,
        DraftsFamily, Family, HistorySummariesCodec, HistorySummariesFamily,
        ImageLabelAuthorityHeadsCodec, ImageLabelAuthorityHeadsFamily, InputGatesCodec,
        InputGatesFamily, ThreadAttributesCodec, ThreadAttributesFamily,
        ThreadCatalogSummariesCodec, ThreadCatalogSummariesFamily, ThreadExecutionsCodec,
        ThreadExecutionsFamily, ThreadTranscriptBuildKey, ThreadUsageCodec, ThreadUsageFamily,
        ThreadsCodec, ThreadsFamily, TranscriptBuildsCodec, TranscriptBuildsFamily,
        TranscriptHeadsCodec, TranscriptHeadsFamily, family_point_limit,
    },
    domain::SyndicDomain,
    draft_piece::{
        DraftEditHistoryFrontiersCodec, DraftEditHistoryFrontiersFamily, DraftPieceRootsCodec,
        DraftPieceRootsFamily, draft_edit_history_frontier_is_authenticated_v1,
    },
    empty_selected_path_digest, root_thread_lineage_digest,
};

const INSPECTION_OPERATION: &str = "pristine-thread inspection";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PristineThreadCandidate {
    source_revision: DomainRevision,
    facts: PristineThreadFacts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PristineThreadAudit {
    Missing,
    Exact(PristineThreadCandidate),
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PristineThreadRemovalAudit {
    Present,
    Removed,
    Collision,
}

impl PristineThreadCandidate {
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.facts.thread.id()
    }

    #[must_use]
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.facts.draft.id()
    }

    #[must_use]
    pub const fn created_at(&self) -> SyndicTimestamp {
        self.facts.draft.created_at()
    }

    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        self.facts.execution.execution()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PristineThreadFacts {
    thread: ThreadRecord,
    index: DraftByThreadRecord,
    draft: DraftRecord,
    root: DraftPieceRootRecordV1,
    history: DraftEditHistoryFrontierV1,
    execution: ThreadExecutionRecord,
    attributes: ThreadAttributesRecord,
    summary: HistorySummaryRecord,
    catalog: ThreadCatalogSummaryRecord,
    input_gate: InputGateRecord,
    image_label_authority: ImageLabelAuthorityHeadV1,
    draft_image_label_protection: DraftImageLabelProtectionHeadV1,
    usage: ThreadUsageRecord,
    transcript_head: TranscriptViewHeadRecord,
    transcript_build: TranscriptBuildRecord,
    activity_head: ActivityQueryHeadRecord,
    binding: BindingRecord,
    binding_head: BindingHeadRecord,
}

struct ValidatePristineThread {
    facts: PristineThreadFacts,
}

struct DeletePristineThread {
    facts: PristineThreadFacts,
}

enum PristineThreadInspection {
    Missing,
    Conflict(&'static str),
    Ineligible,
    Exact(PristineThreadCandidate),
}

impl SyndicStorage {
    pub fn inspect_pristine_thread(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        expected_execution: &ExecutionBinding,
    ) -> Result<Option<PristineThreadCandidate>, SyndicReadError> {
        match self.inspect_pristine_thread_state(store, thread_id, expected_execution)? {
            PristineThreadInspection::Missing => Err(SyndicReadError::Invariant(
                "pristine-thread owner is missing",
            )),
            PristineThreadInspection::Conflict(message) => Err(SyndicReadError::Invariant(message)),
            PristineThreadInspection::Ineligible => Ok(None),
            PristineThreadInspection::Exact(candidate) => Ok(Some(candidate)),
        }
    }

    pub fn audit_pristine_thread(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        expected_execution: &ExecutionBinding,
    ) -> Result<PristineThreadAudit, SyndicReadError> {
        match self.inspect_pristine_thread_state(store, thread_id, expected_execution)? {
            PristineThreadInspection::Missing => Ok(PristineThreadAudit::Missing),
            PristineThreadInspection::Conflict(_) | PristineThreadInspection::Ineligible => {
                Ok(PristineThreadAudit::Conflict)
            }
            PristineThreadInspection::Exact(candidate) => Ok(PristineThreadAudit::Exact(candidate)),
        }
    }

    pub fn audit_pristine_thread_removal(
        &self,
        store: &HomeStore,
        candidate: &PristineThreadCandidate,
    ) -> Result<PristineThreadRemovalAudit, SyndicReadError> {
        let source_revision = self.revision(store)?;
        let audit = removal_audit(self, store, &candidate.facts)?;
        stable_inspection(self, store, source_revision, audit)
    }

    fn inspect_pristine_thread_state(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        expected_execution: &ExecutionBinding,
    ) -> Result<PristineThreadInspection, SyndicReadError> {
        let source_revision = self.revision(store)?;
        let Some(thread) =
            self.point::<ThreadsFamily>(store, thread_id, limit::<ThreadsFamily>())?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Missing,
            );
        };
        let Some(index) =
            self.point::<DraftByThreadFamily>(store, thread_id, limit::<DraftByThreadFamily>())?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict(
                    "pristine-thread current-draft index is missing",
                ),
            );
        };
        let Some(draft) =
            self.point::<DraftsFamily>(store, index.draft_id(), limit::<DraftsFamily>())?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread current draft is missing"),
            );
        };
        let Some(root) = self.point::<DraftPieceRootsFamily>(
            store,
            draft.piece_root().key(),
            limit::<DraftPieceRootsFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread current draft root is missing"),
            );
        };
        let Some(history) = self.point::<DraftEditHistoryFrontiersFamily>(
            store,
            draft.history().key(),
            limit::<DraftEditHistoryFrontiersFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict(
                    "pristine-thread current draft history is missing",
                ),
            );
        };
        let Some(execution) = self.point::<ThreadExecutionsFamily>(
            store,
            thread_id,
            limit::<ThreadExecutionsFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread execution binding is missing"),
            );
        };
        let Some(attributes) = self.point::<ThreadAttributesFamily>(
            store,
            thread_id,
            limit::<ThreadAttributesFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread attributes are missing"),
            );
        };
        let Some(summary) = self.point::<HistorySummariesFamily>(
            store,
            thread_id,
            limit::<HistorySummariesFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread history summary is missing"),
            );
        };
        let Some(catalog) = self.point::<ThreadCatalogSummariesFamily>(
            store,
            thread_id,
            limit::<ThreadCatalogSummariesFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread catalog summary is missing"),
            );
        };
        let Some(input_gate) =
            self.point::<InputGatesFamily>(store, thread_id, limit::<InputGatesFamily>())?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread input gate is missing"),
            );
        };
        let Some(image_label_authority) = self.point::<ImageLabelAuthorityHeadsFamily>(
            store,
            thread_id,
            limit::<ImageLabelAuthorityHeadsFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict(
                    "pristine-thread image-label authority is missing",
                ),
            );
        };
        let Some(draft_image_label_protection) = self
            .point::<DraftImageLabelProtectionHeadsFamily>(
                store,
                thread_id,
                limit::<DraftImageLabelProtectionHeadsFamily>(),
            )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict(
                    "pristine-thread draft-label protection is missing",
                ),
            );
        };
        let Some(usage) =
            self.point::<ThreadUsageFamily>(store, thread_id, limit::<ThreadUsageFamily>())?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread usage is missing"),
            );
        };
        let Some(transcript_head) = self.point::<TranscriptHeadsFamily>(
            store,
            thread_id,
            limit::<TranscriptHeadsFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread transcript head is missing"),
            );
        };
        let transcript_key = ThreadTranscriptBuildKey {
            thread: thread_id,
            generation: TranscriptGeneration::FIRST,
        };
        let Some(transcript_build) = self.point::<TranscriptBuildsFamily>(
            store,
            transcript_key,
            limit::<TranscriptBuildsFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread transcript build is missing"),
            );
        };
        let Some(activity_head) = self.point::<ActivityQueryHeadsFamily>(
            store,
            thread_id,
            limit::<ActivityQueryHeadsFamily>(),
        )?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread activity head is missing"),
            );
        };
        let binding_key = BindingKey {
            thread: thread_id,
            revision: BindingRevision::new(1).expect("initial binding revision is nonzero"),
        };
        let Some(binding) =
            self.point::<BindingsFamily>(store, binding_key, limit::<BindingsFamily>())?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread binding is missing"),
            );
        };
        let Some(binding_head) =
            self.point::<BindingHeadsFamily>(store, thread_id, limit::<BindingHeadsFamily>())?
        else {
            return stable_inspection(
                self,
                store,
                source_revision,
                PristineThreadInspection::Conflict("pristine-thread binding head is missing"),
            );
        };
        let facts = PristineThreadFacts {
            thread,
            index,
            draft,
            root,
            history,
            execution,
            attributes,
            summary,
            catalog,
            input_gate,
            image_label_authority,
            draft_image_label_protection,
            usage,
            transcript_head,
            transcript_build,
            activity_head,
            binding,
            binding_head,
        };
        stable_inspection(self, store, source_revision, ())?;
        if let Err(message) = validate_closure(&facts) {
            return Ok(PristineThreadInspection::Conflict(message));
        }
        if !is_eligible(&facts, expected_execution) {
            return Ok(PristineThreadInspection::Ineligible);
        }
        if !draft_edit_history_frontier_is_authenticated_v1(self, store, &facts.history)? {
            return Err(SyndicReadError::Invariant(
                "pristine-thread current draft history closure is invalid",
            ));
        }
        stable_inspection(
            self,
            store,
            source_revision,
            PristineThreadInspection::Exact(PristineThreadCandidate {
                source_revision,
                facts,
            }),
        )
    }

    #[must_use]
    pub fn validate_pristine_thread(
        &self,
        candidate: PristineThreadCandidate,
    ) -> ValidationContribution {
        self.handle.validation(
            candidate.source_revision,
            ValidatePristineThread {
                facts: candidate.facts,
            },
        )
    }

    #[must_use]
    pub fn delete_pristine_thread(
        &self,
        candidate: PristineThreadCandidate,
    ) -> MutationContribution {
        self.handle.contribution(
            candidate.source_revision,
            DeletePristineThread {
                facts: candidate.facts,
            },
        )
    }
}

impl DomainValidator<SyndicDomain> for ValidatePristineThread {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let facts = &self.facts;
        if !facts_match(reader, facts)? {
            return Err(SyndicMutationError::PristineThreadConflict);
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for DeletePristineThread {
    type Error = SyndicMutationError;
    type Prepared = PristineThreadFacts;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !facts_match(reader, &self.facts)? || !is_created_fallback(&self.facts) {
            return Err(SyndicMutationError::PristineThreadConflict);
        }
        Ok(self.facts)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reserve_pristine_thread_records(reservation)
    }

    fn contribute(
        facts: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        delete_pristine_thread_records(&facts, mutations)
    }
}

fn validate_closure(facts: &PristineThreadFacts) -> Result<(), &'static str> {
    let thread = &facts.thread;
    let draft = &facts.draft;
    if facts.index.thread_id() != thread.id()
        || facts.index.draft_id() != draft.id()
        || facts.index.draft_revision() != draft.revision()
        || facts.index.thread_revision() != thread.revision()
        || thread.current_draft_id() != draft.id()
        || draft.thread_id() != thread.id()
        || facts.root.reference() != draft.piece_root()
        || facts.history.reference() != draft.history()
    {
        return Err("pristine-thread current draft closure disagrees");
    }
    if facts.execution.thread_id() != thread.id()
        || facts.attributes.thread_id() != thread.id()
        || facts.summary.thread_id() != thread.id()
        || facts.catalog.thread_id() != thread.id()
        || facts.input_gate.thread_id() != thread.id()
    {
        return Err("pristine-thread canonical source identities disagree");
    }
    if facts.binding.thread_id() != thread.id()
        || facts.binding_head.thread_id() != thread.id()
        || facts.binding.revision() != facts.binding_head.revision()
        || facts.binding.selected_path()
            != SelectedPathProof::new(
                thread.committed_tail(),
                thread.revision(),
                thread.selected_path_digest(),
            )
        || facts.binding_head.selected_path_digest() != thread.selected_path_digest()
        || facts.binding_head.lifecycle() != facts.binding.state().lifecycle()
        || facts.binding_head.lifecycle() != crate::BindingLifecycle::Unbound
    {
        return Err("pristine-thread binding head is not the initial unbound binding");
    }
    if facts.summary.thread_revision() != thread.revision()
        || facts.summary.committed_tail() != thread.committed_tail()
        || facts.summary.selected_path_digest() != thread.selected_path_digest()
    {
        return Err("pristine-thread history summary is not current");
    }
    let expected_catalog = ThreadCatalogSummaryRecord::from_sources(
        facts.catalog.revision(),
        facts.catalog.title().cloned(),
        thread,
        &facts.execution,
        &facts.attributes,
        &facts.summary,
    );
    if expected_catalog != facts.catalog {
        return Err("pristine-thread catalog summary is not current");
    }
    Ok(())
}

fn facts_match(
    reader: &DomainReader<'_, SyndicDomain>,
    facts: &PristineThreadFacts,
) -> Result<bool, SyndicMutationError> {
    let thread_id = facts.thread.id();
    Ok(
        point_matches::<ThreadsFamily>(reader, &thread_id, &facts.thread)?
            && point_matches::<DraftByThreadFamily>(reader, &thread_id, &facts.index)?
            && point_matches::<DraftsFamily>(reader, &facts.draft.id(), &facts.draft)?
            && point_matches::<DraftPieceRootsFamily>(
                reader,
                &facts.draft.piece_root().key(),
                &facts.root,
            )?
            && point_matches::<DraftEditHistoryFrontiersFamily>(
                reader,
                &facts.draft.history().key(),
                &facts.history,
            )?
            && point_matches::<ThreadExecutionsFamily>(reader, &thread_id, &facts.execution)?
            && point_matches::<ThreadAttributesFamily>(reader, &thread_id, &facts.attributes)?
            && point_matches::<HistorySummariesFamily>(reader, &thread_id, &facts.summary)?
            && point_matches::<ThreadCatalogSummariesFamily>(reader, &thread_id, &facts.catalog)?
            && point_matches::<InputGatesFamily>(reader, &thread_id, &facts.input_gate)?
            && point_matches::<ImageLabelAuthorityHeadsFamily>(
                reader,
                &thread_id,
                &facts.image_label_authority,
            )?
            && point_matches::<DraftImageLabelProtectionHeadsFamily>(
                reader,
                &thread_id,
                &facts.draft_image_label_protection,
            )?
            && point_matches::<ThreadUsageFamily>(reader, &thread_id, &facts.usage)?
            && point_matches::<TranscriptHeadsFamily>(reader, &thread_id, &facts.transcript_head)?
            && point_matches::<TranscriptBuildsFamily>(
                reader,
                &ThreadTranscriptBuildKey {
                    thread: thread_id,
                    generation: facts.transcript_build.generation(),
                },
                &facts.transcript_build,
            )?
            && point_matches::<ActivityQueryHeadsFamily>(reader, &thread_id, &facts.activity_head)?
            && point_matches::<BindingsFamily>(
                reader,
                &BindingKey {
                    thread: thread_id,
                    revision: facts.binding.revision(),
                },
                &facts.binding,
            )?
            && point_matches::<BindingHeadsFamily>(reader, &thread_id, &facts.binding_head)?
            && validate_closure(facts).is_ok()
            && is_eligible(facts, facts.execution.execution()),
    )
}

fn point_matches<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
    expected: &F::Value,
) -> Result<bool, SyndicMutationError>
where
    F::Value: PartialEq,
{
    Ok(crate::mutation::point::<F>(reader, key)?
        .as_ref()
        .is_some_and(|actual| actual == expected))
}

fn removal_audit(
    storage: &SyndicStorage,
    store: &HomeStore,
    facts: &PristineThreadFacts,
) -> Result<PristineThreadRemovalAudit, SyndicReadError> {
    let thread_id = facts.thread.id();
    let mut all_absent = true;
    let mut exact = true;
    macro_rules! inspect {
        ($family:ty, $key:expr, $expected:expr) => {
            match storage.point::<$family>(store, $key, limit::<$family>())? {
                None => exact = false,
                Some(actual) => {
                    all_absent = false;
                    if actual != *$expected {
                        exact = false;
                    }
                }
            }
        };
    }
    inspect!(ThreadsFamily, thread_id, &facts.thread);
    inspect!(DraftByThreadFamily, thread_id, &facts.index);
    inspect!(DraftsFamily, facts.draft.id(), &facts.draft);
    inspect!(
        DraftPieceRootsFamily,
        facts.draft.piece_root().key(),
        &facts.root
    );
    inspect!(
        DraftEditHistoryFrontiersFamily,
        facts.draft.history().key(),
        &facts.history
    );
    inspect!(ThreadExecutionsFamily, thread_id, &facts.execution);
    inspect!(ThreadAttributesFamily, thread_id, &facts.attributes);
    inspect!(HistorySummariesFamily, thread_id, &facts.summary);
    inspect!(ThreadCatalogSummariesFamily, thread_id, &facts.catalog);
    inspect!(InputGatesFamily, thread_id, &facts.input_gate);
    inspect!(
        ImageLabelAuthorityHeadsFamily,
        thread_id,
        &facts.image_label_authority
    );
    inspect!(
        DraftImageLabelProtectionHeadsFamily,
        thread_id,
        &facts.draft_image_label_protection
    );
    inspect!(ThreadUsageFamily, thread_id, &facts.usage);
    inspect!(TranscriptHeadsFamily, thread_id, &facts.transcript_head);
    inspect!(
        TranscriptBuildsFamily,
        ThreadTranscriptBuildKey {
            thread: thread_id,
            generation: facts.transcript_build.generation(),
        },
        &facts.transcript_build
    );
    inspect!(ActivityQueryHeadsFamily, thread_id, &facts.activity_head);
    inspect!(
        BindingsFamily,
        BindingKey {
            thread: thread_id,
            revision: facts.binding.revision(),
        },
        &facts.binding
    );
    inspect!(BindingHeadsFamily, thread_id, &facts.binding_head);
    Ok(if all_absent {
        PristineThreadRemovalAudit::Removed
    } else if exact
        && validate_closure(facts).is_ok()
        && is_eligible(facts, facts.execution.execution())
        && is_created_fallback(facts)
    {
        PristineThreadRemovalAudit::Present
    } else {
        PristineThreadRemovalAudit::Collision
    })
}

fn is_created_fallback(facts: &PristineThreadFacts) -> bool {
    let Some(policy) = DraftEditHistoryPolicyV1::new(
        facts.history.byte_budget(),
        facts.history.retention_policy_revision(),
    ) else {
        return false;
    };
    let expected = CreateThread::ordinary(
        facts.thread.id(),
        facts.draft.id(),
        facts.execution.execution().clone(),
        facts.draft.created_at(),
        policy,
    )
    .records();
    expected.thread == facts.thread
        && expected.image_label_authority_head == facts.image_label_authority
        && expected.draft_image_label_protection_head == facts.draft_image_label_protection
        && expected.execution == facts.execution
        && expected.attributes == facts.attributes
        && expected.usage == facts.usage
        && expected.catalog_summary == facts.catalog
        && expected.draft == facts.draft
        && expected.draft_piece_root == facts.root
        && expected.draft_edit_history == facts.history
        && expected.draft_index == facts.index
        && expected.transcript_head == facts.transcript_head
        && expected.transcript_build.as_ref() == Some(&facts.transcript_build)
        && expected.summary == facts.summary
        && expected.input_gate == facts.input_gate
        && expected.activity_head == facts.activity_head
        && expected.binding == facts.binding
        && expected.binding_head == facts.binding_head
}

fn reserve_pristine_thread_records(
    reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    reservation.reserve_records::<ThreadsCodec>(1)?;
    reservation.reserve_records::<ImageLabelAuthorityHeadsCodec>(1)?;
    reservation.reserve_records::<DraftImageLabelProtectionHeadsCodec>(1)?;
    reservation.reserve_records::<ThreadExecutionsCodec>(1)?;
    reservation.reserve_records::<ThreadAttributesCodec>(1)?;
    reservation.reserve_records::<ThreadUsageCodec>(1)?;
    reservation.reserve_records::<ThreadCatalogSummariesCodec>(1)?;
    reservation.reserve_records::<DraftsCodec>(1)?;
    reservation.reserve_records::<DraftPieceRootsCodec>(1)?;
    reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
    reservation.reserve_records::<DraftByThreadCodec>(1)?;
    reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
    reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
    reservation.reserve_records::<HistorySummariesCodec>(1)?;
    reservation.reserve_records::<InputGatesCodec>(1)?;
    reservation.reserve_records::<ActivityQueryHeadsCodec>(1)?;
    reservation.reserve_records::<BindingsCodec>(1)?;
    reservation.reserve_records::<BindingHeadsCodec>(1)?;
    Ok(())
}

fn delete_pristine_thread_records(
    facts: &PristineThreadFacts,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    let thread_id = facts.thread.id();
    mutations.delete::<ThreadsCodec>(&thread_id)?;
    mutations.delete::<ImageLabelAuthorityHeadsCodec>(&thread_id)?;
    mutations.delete::<DraftImageLabelProtectionHeadsCodec>(&thread_id)?;
    mutations.delete::<ThreadExecutionsCodec>(&thread_id)?;
    mutations.delete::<ThreadAttributesCodec>(&thread_id)?;
    mutations.delete::<ThreadUsageCodec>(&thread_id)?;
    mutations.delete::<ThreadCatalogSummariesCodec>(&thread_id)?;
    mutations.delete::<DraftsCodec>(&facts.draft.id())?;
    mutations.delete::<DraftPieceRootsCodec>(&facts.draft.piece_root().key())?;
    mutations.delete::<DraftEditHistoryFrontiersCodec>(&facts.draft.history().key())?;
    mutations.delete::<DraftByThreadCodec>(&thread_id)?;
    mutations.delete::<TranscriptHeadsCodec>(&thread_id)?;
    mutations.delete::<TranscriptBuildsCodec>(&ThreadTranscriptBuildKey {
        thread: thread_id,
        generation: facts.transcript_build.generation(),
    })?;
    mutations.delete::<HistorySummariesCodec>(&thread_id)?;
    mutations.delete::<InputGatesCodec>(&thread_id)?;
    mutations.delete::<ActivityQueryHeadsCodec>(&thread_id)?;
    mutations.delete::<BindingsCodec>(&BindingKey {
        thread: thread_id,
        revision: facts.binding.revision(),
    })?;
    mutations.delete::<BindingHeadsCodec>(&thread_id)?;
    Ok(())
}

fn is_eligible(facts: &PristineThreadFacts, expected_execution: &ExecutionBinding) -> bool {
    let root_summary = facts.root.reference().summary();
    facts.thread.committed_tail().is_none()
        && facts.thread.selected_path_digest() == empty_selected_path_digest()
        && facts.thread.parent_thread_id().is_none()
        && facts.thread.lineage_ancestor_skip().is_none()
        && facts.thread.lineage_depth() == ThreadLineageDepth::FIRST
        && facts.thread.lineage_digest() == root_thread_lineage_digest(facts.thread.id())
        && facts.thread.context_owner_id().is_none()
        && facts.draft.submission_intent() == DraftSubmissionIntent::Ordinary
        && root_summary.logical_utf8_bytes() == 0
        && root_summary.marker_count() == 0
        && facts.execution.execution() == expected_execution
        && facts.attributes.archive() == ThreadArchiveState::Ordinary
        && facts.attributes.generated_title().is_none()
        && facts.summary.complete()
        && facts.catalog.title().is_none()
        && facts.input_gate.state() == &InputGateState::Idle
        && facts.input_gate.accepted_high_water() == 0
        && facts.input_gate.route_generation_high_water().is_none()
        && facts.input_gate.selected_route().is_none()
        && facts.input_gate.live_steering_count() == 0
        && facts.input_gate.live_next_turn_count() == 0
        && facts.input_gate.live_logical_utf8_bytes() == 0
}

fn limit<F: Family>() -> crate::SyndicPointReadLimit {
    crate::SyndicPointReadLimit::new(family_point_limit::<F>().max_bytes())
        .expect("pristine-thread point-read limit is nonzero")
}

fn stable_inspection<T>(
    storage: &SyndicStorage,
    store: &HomeStore,
    source_revision: DomainRevision,
    value: T,
) -> Result<T, SyndicReadError> {
    if storage.revision(store)? != source_revision {
        return Err(SyndicReadError::ConcurrentChange {
            operation: INSPECTION_OPERATION,
        });
    }
    Ok(value)
}
