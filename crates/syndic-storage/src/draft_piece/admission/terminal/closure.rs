use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader, ReadError};

use crate::{
    SyndicStorage,
    domain::SyndicDomain,
    draft_piece::{
        DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionNodeIdV1,
        DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionNodeKindV1,
        DraftMarkerAdmissionNodesCodec, DraftMarkerAdmissionOwnerV1,
        DraftMarkerAdmissionReceiptKeyV1, DraftMarkerAdmissionReceiptTransitionV1,
        DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionReplayReceiptV1,
        DraftMarkerAdmissionRetainedChargeV1, encoded_head_record_charge,
        encoded_receipt_record_charge,
    },
};

pub(super) const TERMINAL_READ_BYTES: usize = 1_500_000;
const TERMINAL_SOURCE_DOMAIN: &[u8] = b"syndic/draft-marker-admission-terminal/source/v1";
const TERMINAL_TARGET_DOMAIN: &[u8] = b"syndic/draft-marker-admission-terminal/target/v1";

pub(super) struct ExactTerminalClosure {
    pub(super) key: DraftMarkerAdmissionReceiptKeyV1,
    pub(super) receipt: DraftMarkerAdmissionReplayReceiptV1,
    pub(super) encoded_bytes: u64,
}

pub(super) fn validate_compact_terminal_charge(
    head: &DraftMarkerAdmissionHeadV1,
    closure: &ExactTerminalClosure,
) -> Result<(), TerminalClosureError> {
    let head_bytes = encoded_head_record_charge(&head.owner(), head)
        .map_err(|_| TerminalClosureError::Invalid)?;
    let expected_bytes = head_bytes
        .checked_add(closure.encoded_bytes)
        .ok_or(TerminalClosureError::Invalid)?;
    if head.charge() != DraftMarkerAdmissionRetainedChargeV1::new(1, 0, expected_bytes) {
        return Err(TerminalClosureError::Invalid);
    }
    Ok(())
}

pub(super) enum TerminalClosureError {
    Read(ReadError),
    Invalid,
}

impl From<ReadError> for TerminalClosureError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

pub(super) fn read_terminal_closure(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftMarkerAdmissionHeadV1,
) -> Result<ExactTerminalClosure, TerminalClosureError> {
    let page = reader.cursor::<DraftMarkerAdmissionReceiptsCodec>(
        &receipt_range(head.owner()),
        CursorDirection::Forward,
        terminal_receipt_limits(),
    )?;
    exact_terminal_closure(
        head,
        page.records().len(),
        page.has_more(),
        page.records()
            .first()
            .map(|record| (*record.key(), record.value().clone())),
    )
}

pub(super) fn read_terminal_closure_from_store(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    head: &DraftMarkerAdmissionHeadV1,
) -> Result<ExactTerminalClosure, TerminalClosureError> {
    let page = store.read_cursor::<SyndicDomain, DraftMarkerAdmissionReceiptsCodec>(
        &storage.handle,
        &receipt_range(head.owner()),
        CursorDirection::Forward,
        terminal_receipt_limits(),
    )?;
    exact_terminal_closure(
        head,
        page.records().len(),
        page.has_more(),
        page.records()
            .first()
            .map(|record| (*record.key(), record.value().clone())),
    )
}

fn exact_terminal_closure(
    head: &DraftMarkerAdmissionHeadV1,
    record_count: usize,
    has_more: bool,
    record: Option<(
        DraftMarkerAdmissionReceiptKeyV1,
        DraftMarkerAdmissionReplayReceiptV1,
    )>,
) -> Result<ExactTerminalClosure, TerminalClosureError> {
    if record_count != 1 || has_more {
        return Err(TerminalClosureError::Invalid);
    }
    let (key, receipt) = record.ok_or(TerminalClosureError::Invalid)?;
    if !terminal_receipt_is_exact(head, key, &receipt) {
        return Err(TerminalClosureError::Invalid);
    }
    let encoded_bytes =
        encoded_receipt_record_charge(&key, &receipt).map_err(|_| TerminalClosureError::Invalid)?;
    Ok(ExactTerminalClosure {
        key,
        receipt,
        encoded_bytes,
    })
}

pub(crate) fn terminal_receipt_is_exact(
    head: &DraftMarkerAdmissionHeadV1,
    key: DraftMarkerAdmissionReceiptKeyV1,
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
) -> bool {
    key.owner() == head.owner()
        && key.command_id() == receipt.command_id()
        && receipt.owner() == head.owner()
        && receipt.page_ordinal() == head.next_page_ordinal()
        && receipt.request_commitment() == head.request_commitment()
        && receipt.source_head_bytes()
            == terminal_source_closure(head.owner(), key.command_id()).as_ref()
        && receipt.target_head_bytes() == terminal_target_closure(head).as_ref()
        && receipt.source_after() == head.source_root()
        && receipt.target_after() == head.target_root()
        && receipt.retained_predecessor_nodes().is_empty()
        && receipt.transition() == DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup
        && receipt.validate().is_ok()
}

pub(crate) fn terminal_source_closure(
    owner: DraftMarkerAdmissionOwnerV1,
    command: DraftMarkerAdmissionCommandIdV1,
) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity(TERMINAL_SOURCE_DOMAIN.len() + 64);
    bytes.extend_from_slice(TERMINAL_SOURCE_DOMAIN);
    bytes.extend_from_slice(owner.draft_id().as_bytes());
    bytes.extend_from_slice(owner.session_id().as_bytes());
    bytes.extend_from_slice(owner.operation_id().as_bytes());
    bytes.extend_from_slice(command.as_bytes());
    bytes.into_boxed_slice()
}

pub(crate) fn terminal_target_closure(head: &DraftMarkerAdmissionHeadV1) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity(TERMINAL_TARGET_DOMAIN.len() + 64);
    bytes.extend_from_slice(TERMINAL_TARGET_DOMAIN);
    bytes.extend_from_slice(head.request_commitment().as_bytes());
    bytes.extend_from_slice(head.custody_commitment().as_bytes());
    bytes.into_boxed_slice()
}

pub(super) fn terminal_nodes_empty_from_store(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
) -> Result<bool, ReadError> {
    let page = store.read_cursor::<SyndicDomain, DraftMarkerAdmissionNodesCodec>(
        &storage.handle,
        &CursorRange::closed(node_first(owner), node_last(owner)),
        CursorDirection::Forward,
        CursorReadLimits::new(1, TERMINAL_READ_BYTES)
            .expect("draft-marker terminal emptiness limits are nonzero"),
    )?;
    Ok(page.records().is_empty())
}

pub(super) fn node_first(owner: DraftMarkerAdmissionOwnerV1) -> DraftMarkerAdmissionNodeKeyV1 {
    DraftMarkerAdmissionNodeKeyV1::new(
        owner,
        DraftMarkerAdmissionNodeKindV1::Internal,
        DraftMarkerAdmissionNodeIdV1::from_bytes([0; 16]),
    )
}

pub(super) fn node_last(owner: DraftMarkerAdmissionOwnerV1) -> DraftMarkerAdmissionNodeKeyV1 {
    DraftMarkerAdmissionNodeKeyV1::new(
        owner,
        DraftMarkerAdmissionNodeKindV1::Leaf,
        DraftMarkerAdmissionNodeIdV1::from_bytes([u8::MAX; 16]),
    )
}

pub(super) fn receipt_first(
    owner: DraftMarkerAdmissionOwnerV1,
) -> DraftMarkerAdmissionReceiptKeyV1 {
    DraftMarkerAdmissionReceiptKeyV1::new(
        owner,
        DraftMarkerAdmissionCommandIdV1::from_bytes([0; 16]),
    )
}

pub(super) fn receipt_last(owner: DraftMarkerAdmissionOwnerV1) -> DraftMarkerAdmissionReceiptKeyV1 {
    DraftMarkerAdmissionReceiptKeyV1::new(
        owner,
        DraftMarkerAdmissionCommandIdV1::from_bytes([u8::MAX; 16]),
    )
}

pub(super) fn receipt_range(
    owner: DraftMarkerAdmissionOwnerV1,
) -> CursorRange<DraftMarkerAdmissionReceiptKeyV1> {
    CursorRange::closed(receipt_first(owner), receipt_last(owner))
}

fn terminal_receipt_limits() -> CursorReadLimits {
    CursorReadLimits::new(2, TERMINAL_READ_BYTES)
        .expect("draft-marker terminal receipt limits are nonzero")
}
