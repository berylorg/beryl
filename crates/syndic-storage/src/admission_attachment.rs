use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainRegistrationReader,
    DomainRuntimeAttachment, ReadError,
};
use beryl_model::{ImageLabelOrdinal, SyndicThreadId};

use crate::{
    codec::{ScanKey, family_cursor_max_bytes, family_point_limit},
    domain::SyndicDomain,
    draft_piece::{
        DRAFT_MARKER_ADMISSION_MAX_HEADS, DraftMarkerAdmissionCapacityCodec,
        DraftMarkerAdmissionCapacityFamily, DraftMarkerAdmissionCapacityKeyV1,
        DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
        DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodesCodec,
        DraftMarkerAdmissionNodesFamily, DraftMarkerAdmissionOwnerV1,
        DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionReceiptsFamily,
        DraftMarkerAdmissionRetainedChargeV1, DraftMarkerLabelAllocationRangeV1,
        DraftMarkerLabelReadinessDispositionV1, DraftMarkerLabelReadinessRequestAuthorityV1,
    },
};

mod attempt;
mod terminal;

pub(crate) use attempt::{
    DraftMarkerAdmissionAttemptReservation, DraftMarkerAdmissionLiveAuthorityV1,
    DraftMarkerAdmissionPreparedAttempt,
};
pub(crate) use terminal::CancelTransient;

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
    owner: DraftMarkerAdmissionOwnerV1,
    class: ReconstructedHeadClass,
}

struct AttachmentState {
    capacity: DraftMarkerAdmissionRetainedChargeV1,
    heads: Box<[ReconstructedHead]>,
    operations: Vec<OperationReservation>,
    retired: bool,
    #[cfg(feature = "test-faults")]
    allocation_frontiers: Vec<(SyndicThreadId, u64)>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OperationDisposition {
    Open,
    UncertainClosed,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OperationAttempt {
    Idle,
    Prepared(DraftMarkerAdmissionCommandIdV1),
    Dispatched(DraftMarkerAdmissionCommandIdV1),
}

struct OperationReservation {
    owner: DraftMarkerAdmissionOwnerV1,
    frontier: u64,
    attempt: OperationAttempt,
    durable_or_indeterminate: bool,
    disposition: OperationDisposition,
    destination: SyndicThreadId,
    authority: DraftMarkerLabelReadinessRequestAuthorityV1,
    allocation_range: Option<DraftMarkerLabelAllocationRangeV1>,
}

pub(crate) struct DraftMarkerAdmissionAttachment {
    state: Arc<Mutex<AttachmentState>>,
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
            state: Arc::new(Mutex::new(state)),
        })
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn seed_allocation_frontier_for_test(
        &self,
        destination: SyndicThreadId,
        frontier: ImageLabelOrdinal,
    ) -> Result<(), ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        if let Some((_, current)) = state
            .allocation_frontiers
            .iter_mut()
            .find(|(thread_id, _)| *thread_id == destination)
        {
            *current = (*current).max(frontier.get());
        } else {
            if state.allocation_frontiers.len() >= DRAFT_MARKER_ADMISSION_MAX_HEADS as usize {
                return Err(());
            }
            state
                .allocation_frontiers
                .push((destination, frontier.get()));
        }
        Ok(())
    }
}

fn reserve_allocation_if_needed(
    state: &mut AttachmentState,
    owner: DraftMarkerAdmissionOwnerV1,
    authority: &DraftMarkerLabelReadinessRequestAuthorityV1,
    allocation_count: Option<u64>,
) -> Result<(), ()> {
    let existing = state
        .operations
        .iter()
        .find(|entry| entry.owner == owner)
        .and_then(|entry| entry.allocation_range);
    if let Some(existing) = existing {
        return if allocation_count == Some(existing.count()) {
            Ok(())
        } else {
            Err(())
        };
    }
    let requested = allocation_range(state, authority, allocation_count)?;
    if requested.is_some() {
        let entry = state
            .operations
            .iter_mut()
            .find(|entry| entry.owner == owner)
            .ok_or(())?;
        entry.allocation_range = requested;
    }
    Ok(())
}

fn allocation_range(
    state: &AttachmentState,
    authority: &DraftMarkerLabelReadinessRequestAuthorityV1,
    allocation_count: Option<u64>,
) -> Result<Option<DraftMarkerLabelAllocationRangeV1>, ()> {
    match (authority.disposition, allocation_count) {
        (DraftMarkerLabelReadinessDispositionV1::Reuse, None) => Ok(None),
        (DraftMarkerLabelReadinessDispositionV1::Reuse, Some(_)) => Err(()),
        (DraftMarkerLabelReadinessDispositionV1::Allocate, None) => Ok(None),
        (DraftMarkerLabelReadinessDispositionV1::Allocate, Some(0)) => Err(()),
        (DraftMarkerLabelReadinessDispositionV1::Allocate, Some(count)) => {
            let maximum = state
                .operations
                .iter()
                .filter(|entry| entry.destination == authority.session.thread_id())
                .filter_map(|entry| entry.allocation_range.map(|range| range.last().get()))
                .fold(authority.protection.protected_maximum().get(), u64::max);
            #[cfg(feature = "test-faults")]
            let maximum = state
                .allocation_frontiers
                .iter()
                .filter(|(thread_id, _)| *thread_id == authority.session.thread_id())
                .map(|(_, frontier)| *frontier)
                .fold(maximum, u64::max);
            let first =
                ImageLabelOrdinal::new(maximum.checked_add(1).ok_or(())?).map_err(|_| ())?;
            let last = ImageLabelOrdinal::new(
                first
                    .get()
                    .checked_add(count.checked_sub(1).ok_or(())?)
                    .ok_or(())?,
            )
            .map_err(|_| ())?;
            DraftMarkerLabelAllocationRangeV1::new(first, last)
                .map(Some)
                .map_err(|_| ())
        }
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
                state.operations.clear();
                #[cfg(feature = "test-faults")]
                state.allocation_frontiers.clear();
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
        operations: Vec::new(),
        retired: false,
        #[cfg(feature = "test-faults")]
        allocation_frontiers: Vec::new(),
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
            owner: *key,
            class: classify(head),
        });
    }
    if charge != capacity {
        return Err(DraftMarkerAdmissionAttachmentError::CapacityDisagreement);
    }

    Ok(AttachmentState {
        capacity,
        heads: reconstructed.into_boxed_slice(),
        operations: Vec::new(),
        retired: false,
        #[cfg(feature = "test-faults")]
        allocation_frontiers: Vec::new(),
    })
}

fn classify(head: &DraftMarkerAdmissionHeadV1) -> ReconstructedHeadClass {
    match head.lifecycle() {
        DraftMarkerAdmissionLifecycleV1::Ingesting
        | DraftMarkerAdmissionLifecycleV1::Assigning
        | DraftMarkerAdmissionLifecycleV1::Ready
        | DraftMarkerAdmissionLifecycleV1::TerminalCleanup => ReconstructedHeadClass::InertCleanup,
        DraftMarkerAdmissionLifecycleV1::Staging
        | DraftMarkerAdmissionLifecycleV1::Building
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
