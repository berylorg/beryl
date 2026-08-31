use std::{collections::BTreeSet, num::NonZeroU64, sync::Arc};

use beryl_home_store::{HomeProofCommand, HomeProofReceipt, HomeStore, ProofReceiptConsumer};

#[cfg(feature = "test-faults")]
use crate::draft_piece::DraftMarkerAdmissionOwnerV1;
use crate::{
    SyndicStorage,
    admission_attachment::DraftMarkerAdmissionPreparedAttempt,
    codec::{DraftImageLabelProtectionHeadsFamily, ImageLabelAuthorityHeadsFamily},
    draft_piece::{
        DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionHeadsFamily,
        DraftMarkerAdmissionPublicationSeedV1,
    },
};

#[cfg(feature = "test-faults")]
use super::model::{DraftMarkerReadinessSourceAssociationV1, DraftMarkerReadinessWitnessFactoryV1};
use super::{
    model::{
        CanonicalEntry, DraftMarkerLabelReadinessDispositionV1,
        DraftMarkerLabelReadinessPageRequestV1, DraftMarkerLabelReadinessRequestAuthorityV1,
        DraftMarkerReadinessSourceErrorV1, DraftMarkerReadinessSourceSelectorV1,
        PAGE_MAX_ASSOCIATIONS, PAGE_MAX_EVIDENCE_BYTES, PageProtocol,
        SealedDraftMarkerReadinessSourcePageV1, SourceInput, page_correlation, selector_tag,
    },
    proof::{resolve_accepted_preflight, resolve_preflight, session_preflight},
};

pub struct DraftMarkerLabelReadinessPageAttemptV1 {
    command: Option<beryl_home_store::ExecutableHomeProofCommand<PageProtocol>>,
    consumer: Option<ProofReceiptConsumer<PageProtocol>>,
    page: Option<Arc<SealedDraftMarkerReadinessSourcePageV1>>,
    publication: Option<DraftMarkerAdmissionPublicationSeedV1>,
    reservation: Option<DraftMarkerAdmissionPreparedAttempt>,
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
        let (page, _) = self.take_proven_page()?;
        Ok(DraftMarkerLabelReadinessProvenPageV1 { page })
    }

    pub(crate) fn consume_for_submission(
        mut self,
        store: &HomeStore,
        receipt: HomeProofReceipt<PageProtocol>,
    ) -> Result<
        (
            DraftMarkerLabelReadinessProvenPageV1,
            DraftMarkerAdmissionPublicationSeedV1,
            DraftMarkerAdmissionPreparedAttempt,
        ),
        DraftMarkerReadinessSourceErrorV1,
    > {
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
        let (page, publication) = self.take_proven_page()?;
        let reservation = self
            .reservation
            .take()
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        Ok((
            DraftMarkerLabelReadinessProvenPageV1 { page },
            publication,
            reservation,
        ))
    }

    fn take_proven_page(
        &mut self,
    ) -> Result<
        (
            Arc<SealedDraftMarkerReadinessSourcePageV1>,
            DraftMarkerAdmissionPublicationSeedV1,
        ),
        DraftMarkerReadinessSourceErrorV1,
    > {
        let page = self
            .page
            .take()
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        let publication = self
            .publication
            .take()
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        Ok((page, publication))
    }
}

impl SyndicStorage {
    pub fn prepare_draft_marker_label_readiness_page(
        &self,
        store: &HomeStore,
        request: DraftMarkerLabelReadinessPageRequestV1,
    ) -> Result<DraftMarkerLabelReadinessPageAttemptV1, DraftMarkerReadinessSourceErrorV1> {
        let DraftMarkerLabelReadinessPageRequestV1 {
            owner,
            page,
            ordinal,
            eof,
            disposition,
            associations,
            witness_factory,
        } = request;
        let empty_eof = associations.is_empty()
            && ordinal == NonZeroU64::MIN
            && eof
            && disposition == DraftMarkerLabelReadinessDispositionV1::Reuse
            && witness_factory.is_none();
        if associations.len() > PAGE_MAX_ASSOCIATIONS
            || (associations.is_empty() && !empty_eof)
            || associations.windows(2).any(|associations| {
                selector_tag(associations[0].selector) != selector_tag(associations[1].selector)
            })
        {
            return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
        }
        let destination = session_preflight(self, store, owner.draft_id(), owner.session_id())?;
        let label_authority = self
            .point::<ImageLabelAuthorityHeadsFamily>(
                store,
                destination.thread_id(),
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        let protection = self
            .point::<DraftImageLabelProtectionHeadsFamily>(
                store,
                destination.thread_id(),
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        if label_authority.thread_id() != destination.thread_id()
            || protection.thread_id() != destination.thread_id()
            || protection.protected_maximum()
                < label_authority.inherited().max(label_authority.permanent())
        {
            return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
        }
        let home_generation = NonZeroU64::new(
            store
                .health()
                .generation()
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?
                .get(),
        )
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
        let admission_head = self
            .point::<DraftMarkerAdmissionHeadsFamily>(
                store,
                owner,
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?;
        if admission_head
            .as_ref()
            .is_some_and(|head| head.home_generation() != home_generation)
        {
            return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
        }
        let authority = DraftMarkerLabelReadinessRequestAuthorityV1 {
            home_generation,
            label_authority,
            protection,
            session: destination.clone(),
            disposition,
        };
        let mut entries = Vec::with_capacity(associations.len());
        let mut targets = BTreeSet::new();
        for association in associations.iter().copied() {
            if !targets.insert(association.target_marker_id) {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            match association.selector {
                DraftMarkerReadinessSourceSelectorV1::Accepted(source) => {
                    let origin = resolve_accepted_preflight(self, store, source)?;
                    entries.push(CanonicalEntry {
                        target_marker_id: association.target_marker_id,
                        selector: association.selector,
                        label: source.label,
                        asset_id: source.asset_id,
                        accepted_origin: Some(origin),
                    });
                }
                _ => {
                    let (source_thread, occurrence) =
                        resolve_preflight(self, store, association.selector)?;
                    if source_thread != destination.thread_id() {
                        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
                    }
                    entries.push(CanonicalEntry {
                        target_marker_id: association.target_marker_id,
                        selector: association.selector,
                        label: occurrence.label(),
                        asset_id: occurrence.asset_id(),
                        accepted_origin: None,
                    });
                }
            }
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
        let accepted = entries.first().is_some_and(|entry| {
            matches!(
                entry.selector,
                DraftMarkerReadinessSourceSelectorV1::Accepted(_)
            )
        });
        let witness = match (accepted, witness_factory) {
            (true, Some(factory)) => {
                let witness_input = entries
                    .iter()
                    .map(|entry| match entry.selector {
                        DraftMarkerReadinessSourceSelectorV1::Accepted(source) => {
                            Ok((source.asset_reference_set, entry.label, entry.asset_id))
                        }
                        _ => Err(DraftMarkerReadinessSourceErrorV1::Rejected),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Some(factory.build(store, ordinal.get(), eof, witness_input)?)
            }
            (false, None) => None,
            _ => return Err(DraftMarkerReadinessSourceErrorV1::Rejected),
        };
        let expected = page_correlation(ordinal, eof, &entries);
        let revision = store
            .domain_revision(&self.handle)
            .map_err(DraftMarkerReadinessSourceErrorV1::Read)?;
        let allocation_count = match (disposition, eof) {
            (DraftMarkerLabelReadinessDispositionV1::Allocate, true) => {
                let (retained, cursor) = admission_head.as_ref().map_or((0, 0), |head| {
                    (
                        head.target_root().count(),
                        head.ingestion_association_cursor(),
                    )
                });
                let remaining = u64::try_from(entries.len())
                    .ok()
                    .and_then(|count| count.checked_sub(cursor))
                    .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
                Some(
                    retained
                        .checked_add(remaining)
                        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?,
                )
            }
            _ => None,
        };
        let reservation = store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.prepare_attempt(owner, page, ordinal.get(), &authority, allocation_count)
            })
            .map_err(|_| DraftMarkerReadinessSourceErrorV1::Rejected)?
            .map_err(|_| DraftMarkerReadinessSourceErrorV1::Rejected)?;
        let page = Arc::new(SealedDraftMarkerReadinessSourcePageV1 {
            owner,
            page,
            ordinal,
            eof,
            expected,
            entries: entries.into_boxed_slice(),
            authority,
        });
        let publication =
            DraftMarkerAdmissionPublicationSeedV1::from_page(&page, reservation.allocation_range());
        let input = SourceInput {
            page: Arc::clone(&page),
        };
        let source = self.handle.proof_source::<PageProtocol>(revision, input);
        let mut command = HomeProofCommand::new(
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
        if let Some(witness) = witness {
            command
                .add_witness(witness)
                .map_err(|_| DraftMarkerReadinessSourceErrorV1::Build)?;
        }
        let (command, consumer) = command
            .seal()
            .map_err(|_| DraftMarkerReadinessSourceErrorV1::Seal)?;
        Ok(DraftMarkerLabelReadinessPageAttemptV1 {
            command: Some(command),
            consumer: Some(consumer),
            page: Some(page),
            publication: Some(publication),
            reservation: Some(reservation),
        })
    }

    #[cfg(feature = "test-faults")]
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_draft_marker_label_readiness_page_for_test(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        page: DraftMarkerAdmissionCommandIdV1,
        ordinal: NonZeroU64,
        eof: bool,
        associations: Box<[DraftMarkerReadinessSourceAssociationV1]>,
        witness_factory: Option<DraftMarkerReadinessWitnessFactoryV1>,
    ) -> Result<DraftMarkerLabelReadinessPageAttemptV1, DraftMarkerReadinessSourceErrorV1> {
        self.prepare_draft_marker_label_readiness_page(
            store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                page,
                ordinal,
                eof,
                DraftMarkerLabelReadinessDispositionV1::Reuse,
                associations,
                witness_factory,
            ),
        )
    }
}
