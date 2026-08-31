use std::{cmp::Ordering, num::NonZeroU64};

use super::super::{
    DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionLifecycleV1,
    DraftMarkerAdmissionReceiptTransitionV1, DraftMarkerAdmissionReplayReceiptV1,
    DraftMarkerLabelReadinessProvenPageV1,
};
use super::{
    DraftMarkerAdmissionPublicationErrorV1, DraftMarkerAdmissionPublicationSeedV1, PriorPublication,
};

#[derive(Clone, Copy)]
pub(super) struct PageProgression {
    pub(super) association_index: usize,
    pub(super) next_page_ordinal: NonZeroU64,
    pub(super) next_association_cursor: u64,
    pub(super) continues_selected_page: bool,
    pub(super) final_eof: bool,
}

#[derive(Clone, Copy)]
pub(super) enum PageProgressionError {
    Authority,
    Obsolete,
    PageIncomplete,
    Overflow,
}

pub(super) fn page_progression(
    head: Option<&DraftMarkerAdmissionHeadV1>,
    page: &DraftMarkerLabelReadinessProvenPageV1,
) -> Result<PageProgression, PageProgressionError> {
    let page_ordinal = page.sealed_page().ordinal;
    if page.association_count() == 0 {
        if head.is_some() || page_ordinal != NonZeroU64::MIN || !page.sealed_page().eof {
            return Err(PageProgressionError::Authority);
        }
        return Ok(PageProgression {
            association_index: 0,
            next_page_ordinal: NonZeroU64::new(2).expect("empty EOF successor is nonzero"),
            next_association_cursor: 0,
            continues_selected_page: false,
            final_eof: true,
        });
    }
    let (association_index, continues_selected_page) = match head {
        Some(head) => match page_ordinal.cmp(&head.next_page_ordinal()) {
            Ordering::Less => return Err(PageProgressionError::Obsolete),
            Ordering::Greater => return Err(PageProgressionError::PageIncomplete),
            Ordering::Equal => {
                let association_index = usize::try_from(head.ingestion_association_cursor())
                    .map_err(|_| PageProgressionError::Overflow)?;
                (association_index, association_index != 0)
            }
        },
        None => {
            if page_ordinal != NonZeroU64::MIN {
                return Err(PageProgressionError::PageIncomplete);
            }
            (0, false)
        }
    };
    if association_index >= page.association_count() {
        return Err(PageProgressionError::Authority);
    }
    let consumed = association_index
        .checked_add(1)
        .ok_or(PageProgressionError::Overflow)?;
    if consumed == page.association_count() {
        Ok(PageProgression {
            association_index,
            next_page_ordinal: NonZeroU64::new(
                page_ordinal
                    .get()
                    .checked_add(1)
                    .ok_or(PageProgressionError::Overflow)?,
            )
            .ok_or(PageProgressionError::Overflow)?,
            next_association_cursor: 0,
            continues_selected_page,
            final_eof: page.sealed_page().eof,
        })
    } else {
        Ok(PageProgression {
            association_index,
            next_page_ordinal: page_ordinal,
            next_association_cursor: u64::try_from(consumed)
                .map_err(|_| PageProgressionError::Overflow)?,
            continues_selected_page,
            final_eof: false,
        })
    }
}

pub(super) fn authenticate_head(
    seed: &DraftMarkerAdmissionPublicationSeedV1,
    head: &DraftMarkerAdmissionHeadV1,
) -> Result<(), DraftMarkerAdmissionPublicationErrorV1> {
    if head.owner() != seed.owner
        || head.home_generation() != seed.home_generation
        || head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Ingesting
        || head.request_commitment() != seed.request_commitment
        || head.custody_commitment() != seed.custody_commitment
        || head.occurrence_commitment() != seed.occurrence_commitment
        || head.evidence_eof()
        || head.assignment_continuation().is_some()
        || head.cleanup_cursor().is_some()
        || head.source_root().count() != head.target_root().count()
        || head.unassigned_count() != head.target_root().count()
        || head.charge().associations() != head.target_root().count()
    {
        return Err(DraftMarkerAdmissionPublicationErrorV1::Authority);
    }
    Ok(())
}

pub(super) fn authenticate_receipt_closure(
    seed: &DraftMarkerAdmissionPublicationSeedV1,
    head: &DraftMarkerAdmissionHeadV1,
    selected_command: super::super::DraftMarkerAdmissionCommandIdV1,
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
) -> Result<(), DraftMarkerAdmissionPublicationErrorV1> {
    if receipt.owner() != seed.owner
        || receipt.command_id() != selected_command
        || receipt.request_commitment() != seed.request_commitment
        || receipt.source_after() != head.source_root()
        || receipt.target_after() != head.target_root()
        || receipt.transition() != DraftMarkerAdmissionReceiptTransitionV1::Ingestion
    {
        return Err(DraftMarkerAdmissionPublicationErrorV1::Authority);
    }
    Ok(())
}

pub(super) fn authenticate_progression(
    seed: &DraftMarkerAdmissionPublicationSeedV1,
    page: &DraftMarkerLabelReadinessProvenPageV1,
    prior: &PriorPublication,
    progression: PageProgression,
) -> Result<(), DraftMarkerAdmissionPublicationErrorV1> {
    let Some(head) = prior.head.as_ref() else {
        return Ok(());
    };
    let receipt = prior
        .receipt
        .as_ref()
        .ok_or(DraftMarkerAdmissionPublicationErrorV1::Authority)?;
    if progression.continues_selected_page {
        if receipt.command_id() != page.page_identity()
            || receipt.page_ordinal() != page.sealed_page().ordinal
            || receipt.source_head_bytes() != seed.source_head_bytes.as_ref()
            || receipt.target_head_bytes() != seed.target_head_bytes.as_ref()
        {
            return Err(DraftMarkerAdmissionPublicationErrorV1::Collision);
        }
    } else if receipt
        .page_ordinal()
        .get()
        .checked_add(1)
        .and_then(NonZeroU64::new)
        != Some(page.sealed_page().ordinal)
        || head.ingestion_association_cursor() != 0
    {
        return Err(DraftMarkerAdmissionPublicationErrorV1::Authority);
    }
    Ok(())
}
