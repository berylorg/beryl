use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

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
        DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
        DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodesCodec,
        DraftMarkerAdmissionNodesFamily, DraftMarkerAdmissionOwnerV1,
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
    operations: Vec<OperationReservation>,
    retired: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OperationDisposition {
    Open,
    UncertainClosed,
}

struct OperationReservation {
    owner: DraftMarkerAdmissionOwnerV1,
    frontier: u64,
    active_attempt: Option<DraftMarkerAdmissionCommandIdV1>,
    disposition: OperationDisposition,
}

pub(crate) struct DraftMarkerAdmissionAttemptReservation {
    pub(crate) was_present: bool,
}

pub(crate) struct DraftMarkerAdmissionPreparedAttempt {
    state: Option<Arc<Mutex<AttachmentState>>>,
    owner: DraftMarkerAdmissionOwnerV1,
    attempt: DraftMarkerAdmissionCommandIdV1,
    was_present: bool,
}

impl DraftMarkerAdmissionPreparedAttempt {
    pub(crate) fn disarm(mut self) -> DraftMarkerAdmissionAttemptReservation {
        self.state = None;
        DraftMarkerAdmissionAttemptReservation {
            was_present: self.was_present,
        }
    }
}

impl Drop for DraftMarkerAdmissionPreparedAttempt {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        let Ok(mut state) = state.lock() else {
            return;
        };
        let Some(index) = state
            .operations
            .iter()
            .position(|entry| entry.owner == self.owner)
        else {
            return;
        };
        if state.operations[index].active_attempt != Some(self.attempt) {
            return;
        }
        if self.was_present {
            state.operations[index].active_attempt = None;
        } else {
            state.operations.remove(index);
        }
    }
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

    pub(crate) fn prepare_attempt(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        attempt: DraftMarkerAdmissionCommandIdV1,
        frontier: u64,
    ) -> Result<DraftMarkerAdmissionPreparedAttempt, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        if let Some(operation) = state
            .operations
            .iter_mut()
            .find(|entry| entry.owner == owner)
        {
            if operation.disposition != OperationDisposition::Open
                || operation.active_attempt.is_some()
            {
                return Err(());
            }
            operation.active_attempt = Some(attempt);
            return Ok(DraftMarkerAdmissionPreparedAttempt {
                state: Some(Arc::clone(&self.state)),
                owner,
                attempt,
                was_present: true,
            });
        }
        if state.operations.len() >= DRAFT_MARKER_ADMISSION_MAX_HEADS as usize {
            return Err(());
        }
        state.operations.push(OperationReservation {
            owner,
            frontier,
            active_attempt: Some(attempt),
            disposition: OperationDisposition::Open,
        });
        Ok(DraftMarkerAdmissionPreparedAttempt {
            state: Some(Arc::clone(&self.state)),
            owner,
            attempt,
            was_present: false,
        })
    }

    pub(crate) fn finish_submission(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        attempt: DraftMarkerAdmissionCommandIdV1,
        retain_operation: bool,
        uncertain_closed: bool,
        frontier: u64,
    ) -> Result<(), ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let index = state
            .operations
            .iter()
            .position(|entry| entry.owner == owner)
            .ok_or(())?;
        if state.operations[index].active_attempt != Some(attempt) {
            return Err(());
        }
        if retain_operation {
            let operation = &mut state.operations[index];
            operation.active_attempt = None;
            operation.frontier = operation.frontier.max(frontier);
            if uncertain_closed {
                operation.disposition = OperationDisposition::UncertainClosed;
            }
        } else {
            state.operations.remove(index);
        }
        Ok(())
    }

    pub(crate) fn resolve_submission(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        retain_operation: bool,
        uncertain_closed: bool,
        frontier: u64,
    ) -> Result<(), ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let Some(index) = state
            .operations
            .iter()
            .position(|entry| entry.owner == owner)
        else {
            return Ok(());
        };
        if state.operations[index].active_attempt.is_some() {
            return Err(());
        }
        if retain_operation {
            let operation = &mut state.operations[index];
            operation.frontier = operation.frontier.max(frontier);
            operation.disposition = if uncertain_closed {
                OperationDisposition::UncertainClosed
            } else {
                OperationDisposition::Open
            };
        } else {
            state.operations.remove(index);
        }
        Ok(())
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
        operations: Vec::new(),
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
