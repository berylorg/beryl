use std::{error::Error, fmt};

use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, MutationContribution, ReconciliationReservation,
};
use beryl_model::DomainRevision;
use sha2::{Digest, Sha256};

use crate::domain::{SyndicDomain, SyndicStorage};
use crate::mutation::point;
use crate::{SyndicMutationError, SyndicReadError};

use super::*;

#[derive(Debug)]
pub enum DraftMutationStagingErrorV1 {
    Read(SyndicReadError),
    Invalid,
    Overflow,
    Invariant,
}

impl fmt::Display for DraftMutationStagingErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "{error}"),
            Self::Invalid => formatter.write_str("invalid draft mutation staging request"),
            Self::Overflow => formatter.write_str("draft mutation staging total overflow"),
            Self::Invariant => formatter.write_str("draft mutation staging invariant failure"),
        }
    }
}

impl Error for DraftMutationStagingErrorV1 {}

impl From<SyndicReadError> for DraftMutationStagingErrorV1 {
    fn from(value: SyndicReadError) -> Self {
        Self::Read(value)
    }
}

#[derive(Clone)]
pub struct PreparedDraftMutationStagingCommandV1 {
    source_head: Option<DraftMutationStagingHeadV1>,
    target_head: DraftMutationStagingHeadV1,
    source_session: DraftEditorCandidateSessionV1,
    target_session: Option<DraftEditorCandidateSessionV1>,
    page: Option<DraftMutationStagingPageV1>,
    receipt: DraftMutationStagingProgressReceiptV1,
}

#[derive(Clone)]
pub struct PreparedDraftMutationTransferV1 {
    source_head: DraftMutationStagingHeadV1,
    target_head: DraftMutationStagingHeadV1,
    receipt: DraftMutationStagingProgressReceiptV1,
    source_session: DraftEditorCandidateSessionV1,
    target_session: DraftEditorCandidateSessionV1,
    prepared_edit: PreparedDraftPieceEditV1,
    build: DraftPieceBuildRecordV1,
    build_receipt: DraftPieceBuildProgressReceiptV1,
}

#[derive(Clone)]
pub struct PreparedDraftPieceStagingPageV1 {
    staging_head: DraftMutationStagingHeadV1,
    staging_page: DraftMutationStagingPageV1,
    expected_build: DraftPieceBuildRecordV1,
    expected_session: DraftEditorCandidateSessionV1,
    target_build: DraftPieceBuildRecordV1,
    target_receipt: DraftPieceBuildProgressReceiptV1,
    target_session: DraftEditorCandidateSessionV1,
    fragments: Box<[DraftPieceBuildFragmentV1]>,
}

impl PreparedDraftPieceStagingPageV1 {
    pub const fn lane(&self) -> DraftMutationStagingLaneV1 {
        self.staging_page.key().lane()
    }
    pub const fn page_ordinal(&self) -> u64 {
        self.staging_page.key().ordinal()
    }
    pub fn fragment_count(&self) -> usize {
        self.fragments.len()
    }
}

impl PreparedDraftMutationTransferV1 {
    pub fn target_head(&self) -> &DraftMutationStagingHeadV1 {
        &self.target_head
    }
    pub fn build(&self) -> &DraftPieceBuildRecordV1 {
        &self.build
    }
    pub fn prepared_edit(&self) -> &PreparedDraftPieceEditV1 {
        &self.prepared_edit
    }
}

impl PreparedDraftMutationStagingCommandV1 {
    pub fn target_head(&self) -> &DraftMutationStagingHeadV1 {
        &self.target_head
    }
    pub const fn target_session(&self) -> Option<&DraftEditorCandidateSessionV1> {
        self.target_session.as_ref()
    }
    pub const fn receipt(&self) -> &DraftMutationStagingProgressReceiptV1 {
        &self.receipt
    }
    pub const fn page(&self) -> Option<&DraftMutationStagingPageV1> {
        self.page.as_ref()
    }
}

#[derive(Clone)]
struct StagingMutation {
    prepared: PreparedDraftMutationStagingCommandV1,
}

#[derive(Clone)]
struct TransferMutation {
    prepared: PreparedDraftMutationTransferV1,
}

#[derive(Clone)]
struct StageDurablePageMutation {
    prepared: PreparedDraftPieceStagingPageV1,
}

mod digest;
mod integrity;
mod mutations;
mod prepare_begin_page;
mod prepare_finish;
mod prepare_terminal;
mod status;
mod transfer_builder;

use digest::*;
use integrity::*;
use terminal::*;

mod terminal;

pub(crate) fn draft_mutation_staging_head_is_locally_exact(
    head: &DraftMutationStagingHeadV1,
) -> bool {
    digest::draft_mutation_staging_head_is_locally_exact(head)
}

pub(crate) fn draft_mutation_staging_page_is_locally_exact(
    page: &DraftMutationStagingPageV1,
) -> bool {
    digest::draft_mutation_staging_page_is_locally_exact(page)
}

pub(crate) fn draft_mutation_staging_receipt_is_locally_exact(
    receipt: &DraftMutationStagingProgressReceiptV1,
) -> bool {
    digest::draft_mutation_staging_receipt_is_locally_exact(receipt)
}

#[cfg(feature = "test-faults")]
pub(crate) fn head_digest(
    head: DraftMutationStagingHeadV1,
) -> Result<DraftPieceDigestV1, DraftMutationStagingErrorV1> {
    digest::head_digest(head)
}

pub(super) fn authenticate_staging_head_reader(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftMutationStagingHeadV1,
) -> Result<DraftMutationStagingProgressReceiptV1, SyndicMutationError> {
    integrity::authenticate_staging_head_reader(reader, head)
}
