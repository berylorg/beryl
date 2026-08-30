use std::{collections::BTreeSet, num::NonZeroU64, sync::Arc};

use beryl_home_store::{HomeProofCommand, HomeProofReceipt, HomeStore, ProofReceiptConsumer};

use crate::{
    SyndicStorage,
    draft_piece::{DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionOwnerV1},
};

use super::{
    model::{
        CanonicalEntry, DraftMarkerReadinessSourceAssociationV1, DraftMarkerReadinessSourceErrorV1,
        PAGE_MAX_ASSOCIATIONS, PAGE_MAX_EVIDENCE_BYTES, PageProtocol,
        SealedDraftMarkerReadinessSourcePageV1, SourceInput, page_correlation, selector_tag,
    },
    proof::{resolve_preflight, session_preflight},
};

pub struct DraftMarkerLabelReadinessPageAttemptV1 {
    command: Option<beryl_home_store::ExecutableHomeProofCommand<PageProtocol>>,
    consumer: Option<ProofReceiptConsumer<PageProtocol>>,
    page: Option<Arc<SealedDraftMarkerReadinessSourcePageV1>>,
}

pub struct DraftMarkerLabelReadinessProvenPageV1 {
    page: Arc<SealedDraftMarkerReadinessSourcePageV1>,
}

impl DraftMarkerLabelReadinessProvenPageV1 {
    pub(crate) fn sealed_page(&self) -> &SealedDraftMarkerReadinessSourcePageV1 {
        &self.page
    }

    pub(crate) fn page_identity(&self) -> DraftMarkerAdmissionCommandIdV1 {
        self.page.page
    }

    pub(crate) fn association_count(&self) -> usize {
        self.page.entries.len()
    }
}

impl DraftMarkerLabelReadinessPageAttemptV1 {
    #[cfg(feature = "test-faults")]
    pub fn expected_source_correlation_for_test(&self) -> [u8; 32] {
        self.page
            .as_ref()
            .expect("sealed readiness attempt retains its page")
            .expected
    }

    pub fn take_command(
        &mut self,
    ) -> Option<beryl_home_store::ExecutableHomeProofCommand<PageProtocol>> {
        self.command.take()
    }

    pub fn consume(
        mut self,
        store: &HomeStore,
        receipt: HomeProofReceipt<PageProtocol>,
    ) -> Result<DraftMarkerLabelReadinessProvenPageV1, DraftMarkerReadinessSourceErrorV1> {
        if self.command.is_some() {
            return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
        }
        let consumer = self
            .consumer
            .take()
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        store
            .consume_proof_receipt(consumer, receipt)
            .map_err(DraftMarkerReadinessSourceErrorV1::Receipt)?;
        let page = self
            .page
            .take()
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        Ok(DraftMarkerLabelReadinessProvenPageV1 { page })
    }
}

impl SyndicStorage {
    pub fn prepare_draft_marker_label_readiness_source_page(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        page: DraftMarkerAdmissionCommandIdV1,
        ordinal: NonZeroU64,
        eof: bool,
        associations: Box<[DraftMarkerReadinessSourceAssociationV1]>,
    ) -> Result<DraftMarkerLabelReadinessPageAttemptV1, DraftMarkerReadinessSourceErrorV1> {
        if associations.len() > PAGE_MAX_ASSOCIATIONS
            || (associations.is_empty() && !eof)
            || associations.windows(2).any(|associations| {
                selector_tag(associations[0].selector) != selector_tag(associations[1].selector)
            })
        {
            return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
        }
        let destination = session_preflight(self, store, owner.draft_id(), owner.session_id())?;
        let mut entries = Vec::with_capacity(associations.len());
        let mut targets = BTreeSet::new();
        for association in associations.iter().copied() {
            if !targets.insert(association.target_marker_id) {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            let (source_thread, occurrence) = resolve_preflight(self, store, association.selector)?;
            if source_thread != destination.thread_id() {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            entries.push(CanonicalEntry {
                target_marker_id: association.target_marker_id,
                selector: association.selector,
                label: occurrence.label(),
                asset_id: occurrence.asset_id(),
            });
        }
        entries.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| left.evidence_bytes().cmp(&right.evidence_bytes()))
        });
        let evidence_bytes = entries
            .iter()
            .try_fold(0_usize, |total, entry| {
                total.checked_add(entry.evidence_bytes().len())
            })
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        if evidence_bytes > PAGE_MAX_EVIDENCE_BYTES {
            return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
        }
        let expected = page_correlation(ordinal, eof, &entries);
        let revision = store
            .domain_revision(&self.handle)
            .map_err(DraftMarkerReadinessSourceErrorV1::Read)?;
        let page = Arc::new(SealedDraftMarkerReadinessSourcePageV1 {
            owner,
            page,
            ordinal,
            eof,
            expected,
            entries: entries.into_boxed_slice(),
        });
        let input = SourceInput {
            page: Arc::clone(&page),
        };
        let source = self.handle.proof_source::<PageProtocol>(revision, input);
        let command = HomeProofCommand::new(
            store
                .health()
                .generation()
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?,
            store
                .home_revision()
                .map_err(DraftMarkerReadinessSourceErrorV1::Read)?,
            source,
        )
        .map_err(|_| DraftMarkerReadinessSourceErrorV1::Build)?;
        let (command, consumer) = command
            .seal()
            .map_err(|_| DraftMarkerReadinessSourceErrorV1::Seal)?;
        Ok(DraftMarkerLabelReadinessPageAttemptV1 {
            command: Some(command),
            consumer: Some(consumer),
            page: Some(page),
        })
    }
}
