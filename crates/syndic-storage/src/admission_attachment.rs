use std::{error::Error, fmt, sync::Mutex};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainRegistrationReader,
    DomainRuntimeAttachment, ReadError,
};

use crate::{
    codec::{ScanKey, family_cursor_max_bytes, family_point_limit},
    domain::SyndicDomain,
    draft_piece::{
        DRAFT_MARKER_ADMISSION_MAX_HEADS, DraftMarkerAdmissionCapacityCodec,
        DraftMarkerAdmissionCapacityFamily, DraftMarkerAdmissionCapacityKeyV1,
        DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionHeadsCodec,
        DraftMarkerAdmissionHeadsFamily, DraftMarkerAdmissionLifecycleV1,
        DraftMarkerAdmissionNodesCodec, DraftMarkerAdmissionNodesFamily,
        DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionReceiptsFamily,
        DraftMarkerAdmissionRetainedChargeV1,
    },
};

#[derive(Debug)]
pub(crate) enum DraftMarkerAdmissionAttachmentError {
    Read(ReadError),
    TooManyHeads,
    HeadKeyMismatch,
    ChargeOverflow,
    CapacityDisagreement,
    UnexpectedRecordWithoutCapacity,
}

impl fmt::Display for DraftMarkerAdmissionAttachmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(
                formatter,
                "draft-marker admission reconstruction read failed: {error}"
            ),
            Self::TooManyHeads => formatter
                .write_str("draft-marker admission reconstruction found more than 64 heads"),
            Self::HeadKeyMismatch => {
                formatter.write_str("draft-marker admission head key disagrees with its owner")
            }
            Self::ChargeOverflow => {
                formatter.write_str("draft-marker admission reconstructed charge overflowed")
            }
            Self::CapacityDisagreement => formatter
                .write_str("draft-marker admission capacity disagrees with reconstructed heads"),
            Self::UnexpectedRecordWithoutCapacity => formatter
                .write_str("draft-marker admission records exist without the capacity singleton"),
        }
    }
}

impl Error for DraftMarkerAdmissionAttachmentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum ReconstructedHeadClass {
    InertCleanup,
    JointCleanup,
}

struct ReconstructedHead {
    class: ReconstructedHeadClass,
}

struct AttachmentState {
    capacity: DraftMarkerAdmissionRetainedChargeV1,
    heads: Box<[ReconstructedHead]>,
    retired: bool,
}

pub(crate) struct DraftMarkerAdmissionAttachment {
    state: Mutex<AttachmentState>,
}

impl DraftMarkerAdmissionAttachment {
    pub(crate) fn reconstruct(
        reader: &DomainRegistrationReader<'_, SyndicDomain>,
    ) -> Result<Self, DraftMarkerAdmissionAttachmentError> {
        let capacity = reader
            .point::<DraftMarkerAdmissionCapacityCodec>(
                &DraftMarkerAdmissionCapacityKeyV1,
                family_point_limit::<DraftMarkerAdmissionCapacityFamily>(),
            )
            .map_err(DraftMarkerAdmissionAttachmentError::Read)?;

        let state = match capacity {
            Some(capacity) => reconstruct_populated(reader, capacity.charge())?,
            None => reconstruct_empty(reader)?,
        };
        Ok(Self {
            state: Mutex::new(state),
        })
    }
}

impl DomainRuntimeAttachment for DraftMarkerAdmissionAttachment {
    fn retire(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if !state.retired {
                let _has_joint_cleanup = state
                    .heads
                    .iter()
                    .any(|head| matches!(head.class, ReconstructedHeadClass::JointCleanup));
                state.heads = Box::new([]);
                state.capacity = DraftMarkerAdmissionRetainedChargeV1::ZERO;
                state.retired = true;
            }
        }
    }
}

fn reconstruct_empty(
    reader: &DomainRegistrationReader<'_, SyndicDomain>,
) -> Result<AttachmentState, DraftMarkerAdmissionAttachmentError> {
    let heads = reader
        .cursor::<DraftMarkerAdmissionHeadsCodec>(
            &full_range::<DraftMarkerAdmissionHeadsCodec>(),
            CursorDirection::Forward,
            first_record_limits(family_cursor_max_bytes::<DraftMarkerAdmissionHeadsFamily>()),
        )
        .map_err(DraftMarkerAdmissionAttachmentError::Read)?;
    let nodes = reader
        .cursor::<DraftMarkerAdmissionNodesCodec>(
            &full_range::<DraftMarkerAdmissionNodesCodec>(),
            CursorDirection::Forward,
            first_record_limits(family_cursor_max_bytes::<DraftMarkerAdmissionNodesFamily>()),
        )
        .map_err(DraftMarkerAdmissionAttachmentError::Read)?;
    let receipts = reader
        .cursor::<DraftMarkerAdmissionReceiptsCodec>(
            &full_range::<DraftMarkerAdmissionReceiptsCodec>(),
            CursorDirection::Forward,
            first_record_limits(family_cursor_max_bytes::<DraftMarkerAdmissionReceiptsFamily>()),
        )
        .map_err(DraftMarkerAdmissionAttachmentError::Read)?;
    if !heads.records().is_empty() || !nodes.records().is_empty() || !receipts.records().is_empty()
    {
        return Err(DraftMarkerAdmissionAttachmentError::UnexpectedRecordWithoutCapacity);
    }
    Ok(AttachmentState {
        capacity: DraftMarkerAdmissionRetainedChargeV1::ZERO,
        heads: Box::new([]),
        retired: false,
    })
}

fn reconstruct_populated(
    reader: &DomainRegistrationReader<'_, SyndicDomain>,
    capacity: DraftMarkerAdmissionRetainedChargeV1,
) -> Result<AttachmentState, DraftMarkerAdmissionAttachmentError> {
    let limits = CursorReadLimits::new(
        usize::try_from(DRAFT_MARKER_ADMISSION_MAX_HEADS + 1)
            .expect("draft-marker admission head sentinel count fits usize"),
        family_cursor_max_bytes::<DraftMarkerAdmissionHeadsFamily>()
            .checked_mul(
                usize::try_from(DRAFT_MARKER_ADMISSION_MAX_HEADS + 1)
                    .expect("draft-marker admission head sentinel count fits usize"),
            )
            .expect("draft-marker admission head sentinel byte bound fits usize"),
    )
    .expect("draft-marker admission head sentinel limits are nonzero");
    let page = reader
        .cursor::<DraftMarkerAdmissionHeadsCodec>(
            &full_range::<DraftMarkerAdmissionHeadsCodec>(),
            CursorDirection::Forward,
            limits,
        )
        .map_err(DraftMarkerAdmissionAttachmentError::Read)?;
    if page.records().len() > DRAFT_MARKER_ADMISSION_MAX_HEADS as usize || page.has_more() {
        return Err(DraftMarkerAdmissionAttachmentError::TooManyHeads);
    }

    let mut reconstructed = Vec::with_capacity(page.records().len());
    let mut charge = DraftMarkerAdmissionRetainedChargeV1::ZERO;
    for record in page.records() {
        let (key, head) = (record.key(), record.value());
        if *key != head.owner() {
            return Err(DraftMarkerAdmissionAttachmentError::HeadKeyMismatch);
        }
        charge = charge
            .checked_add(head.charge())
            .ok_or(DraftMarkerAdmissionAttachmentError::ChargeOverflow)?;
        reconstructed.push(ReconstructedHead {
            class: classify(head),
        });
    }
    if charge != capacity {
        return Err(DraftMarkerAdmissionAttachmentError::CapacityDisagreement);
    }

    Ok(AttachmentState {
        capacity,
        heads: reconstructed.into_boxed_slice(),
        retired: false,
    })
}

fn classify(head: &DraftMarkerAdmissionHeadV1) -> ReconstructedHeadClass {
    match head.lifecycle() {
        DraftMarkerAdmissionLifecycleV1::Ingesting
        | DraftMarkerAdmissionLifecycleV1::Assigning
        | DraftMarkerAdmissionLifecycleV1::Ready => ReconstructedHeadClass::InertCleanup,
        DraftMarkerAdmissionLifecycleV1::Building
        | DraftMarkerAdmissionLifecycleV1::TerminalCleanup
        | DraftMarkerAdmissionLifecycleV1::Settled => ReconstructedHeadClass::JointCleanup,
    }
}

fn full_range<R>() -> CursorRange<R::Key>
where
    R: beryl_home_store::RecordCodec<SyndicDomain>,
    R::Key: ScanKey,
{
    CursorRange::closed(R::Key::first(), R::Key::last())
}

fn first_record_limits(max_bytes: usize) -> CursorReadLimits {
    CursorReadLimits::new(1, max_bytes).expect("codec-derived first-record limits are nonzero")
}
