use std::{error::Error, fmt, num::NonZeroU64};

use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, DomainCallbackError, DomainCallbackSource,
    DomainMutation, DomainReader, HomeStore, MutationBuildError, MutationBuilder, ReadError,
    ReconciliationHandle, ReconciliationReservation, ReconciliationResolution,
};
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicThreadId};

use crate::{
    DraftEditorCandidateSessionV1, DraftImageLabelProtectionHeadV1, ImageLabelAuthorityHeadV1,
    SyndicReadError, SyndicStorage,
    admission_attachment::{
        DraftMarkerAdmissionLiveAuthorityV1, DraftMarkerAdmissionPreparedAttempt,
    },
    codec::{
        DraftImageLabelProtectionHeadsFamily, ImageLabelAuthorityHeadsFamily, family_point_limit,
    },
    domain::SyndicDomain,
    draft_piece::{
        DraftEditorCandidateSessionRecordKeyV1, DraftEditorCandidateSessionRecordV1,
        DraftEditorCandidateSessionsFamily,
    },
};

use super::index::{
    DraftMarkerAdmissionIndexPreparationErrorV1, PreparedDraftMarkerAdmissionAssignmentV1,
    PreparedDraftMarkerAdmissionIndexSuccessorV1, prepare_draft_marker_admission_assignment_v1,
};
use super::readiness_source::request_authority_exact_read_bytes;

mod mutation;
#[cfg(feature = "test-faults")]
mod test_fixture;

use mutation::{AssignmentFailureClass, AssignmentMutation, classify_assignment_failure};

use super::{
    DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT,
    DraftMarkerAdmissionAssignmentContinuationV1, DraftMarkerAdmissionCapacityCodec,
    DraftMarkerAdmissionCapacityFamily, DraftMarkerAdmissionCapacityKeyV1,
    DraftMarkerAdmissionCapacityV1, DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionDigestV1,
    DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
    DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodesCodec, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionReceiptKeyV1, DraftMarkerAdmissionReceiptTransitionV1,
    DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionReceiptsFamily,
    DraftMarkerAdmissionReplayReceiptV1, DraftMarkerAdmissionRetainedChargeV1,
    DraftMarkerAdmissionRootV1, DraftMarkerAdmissionSchemaErrorV1,
    DraftMarkerLabelAllocationRangeV1, DraftMarkerLabelReadinessDispositionV1,
    canonical_empty_draft_marker_admission_root_v1,
    checked_draft_marker_admission_command_charge_v1, encoded_capacity_record_charge,
    encoded_head_record_charge, encoded_receipt_record_charge,
};
#[cfg(feature = "test-faults")]
pub use test_fixture::DraftMarkerAssignedAssociationV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerLabelAssignmentRefusalV1 {
    Unavailable,
    Obsolete,
    Rejected,
}

#[derive(Debug)]
pub enum DraftMarkerLabelAssignmentErrorV1 {
    Read(SyndicReadError),
    Unavailable,
    Rejected,
}

impl fmt::Display for DraftMarkerLabelAssignmentErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "draft-marker assignment read failed: {error}"),
            Self::Unavailable => formatter.write_str("draft-marker assignment is unavailable"),
            Self::Rejected => formatter.write_str("draft-marker assignment was rejected"),
        }
    }
}

impl Error for DraftMarkerLabelAssignmentErrorV1 {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            _ => None,
        }
    }
}

pub struct DraftMarkerLabelAssignmentFlightV1 {
    state: AssignmentFlightState,
}

enum AssignmentFlightState {
    Ready {
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
        reservation: DraftMarkerAdmissionPreparedAttempt,
        authority: DraftMarkerAdmissionLiveAuthorityV1,
        retained_limits: super::DraftMarkerAdmissionLimitsV1,
        command_limit: AssignmentCommandLimit,
    },
    Reconciling {
        owner: DraftMarkerAdmissionOwnerV1,
        handle: ReconciliationHandle,
        retry_failed: bool,
        authority: DraftMarkerAdmissionLiveAuthorityV1,
    },
}

#[derive(Clone, Copy)]
enum AssignmentCommandLimit {
    Exact(u64),
    #[cfg(feature = "test-faults")]
    BeforeAuthorityReads,
}

pub enum DraftMarkerLabelAssignmentOutcomeV1 {
    Advanced {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    },
    Ready {
        proof: DraftMarkerLabelReadinessProofV1,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    },
    Retryable,
    Refused(DraftMarkerLabelAssignmentRefusalV1),
    Collision,
    ReconciliationPending(DraftMarkerLabelAssignmentFlightV1),
}

pub struct DraftMarkerLabelReadinessProofV1 {
    home_generation: NonZeroU64,
    owner: DraftMarkerAdmissionOwnerV1,
    label_authority: ImageLabelAuthorityHeadV1,
    protection: DraftImageLabelProtectionHeadV1,
    session: DraftEditorCandidateSessionV1,
    disposition: DraftMarkerLabelReadinessDispositionV1,
    occurrence_commitment: DraftMarkerAdmissionDigestV1,
    assigned_target_root: DraftMarkerAdmissionRootV1,
    allocation_range: Option<DraftMarkerLabelAllocationRangeV1>,
}

impl DraftMarkerLabelReadinessProofV1 {
    pub const fn home_generation(&self) -> NonZeroU64 {
        self.home_generation
    }

    pub const fn owner(&self) -> DraftMarkerAdmissionOwnerV1 {
        self.owner
    }

    pub const fn destination_thread_id(&self) -> SyndicThreadId {
        self.label_authority.thread_id()
    }

    pub const fn label_authority(&self) -> ImageLabelAuthorityHeadV1 {
        self.label_authority
    }

    pub const fn protection(&self) -> DraftImageLabelProtectionHeadV1 {
        self.protection
    }

    pub const fn session(&self) -> &DraftEditorCandidateSessionV1 {
        &self.session
    }

    pub const fn disposition(&self) -> DraftMarkerLabelReadinessDispositionV1 {
        self.disposition
    }

    pub const fn occurrence_commitment(&self) -> DraftMarkerAdmissionDigestV1 {
        self.occurrence_commitment
    }

    pub const fn assigned_target_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.assigned_target_root
    }

    pub const fn allocation_range(&self) -> Option<DraftMarkerLabelAllocationRangeV1> {
        self.allocation_range
    }
}

impl SyndicStorage {
    pub fn prepare_draft_marker_label_assignment(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
    ) -> Result<DraftMarkerLabelAssignmentFlightV1, DraftMarkerLabelAssignmentErrorV1> {
        let head = self
            .point::<DraftMarkerAdmissionHeadsFamily>(
                store,
                owner,
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        let generation = store
            .health()
            .generation()
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Unavailable)?;
        if head.home_generation().get() != generation.get()
            || head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Assigning
        {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        let (reservation, authority) = store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.prepare_assignment_attempt(owner, command)
            })
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        if authority.authority.home_generation != head.home_generation()
            || authority.authority.request_commitment() != head.request_commitment()
            || authority.authority.custody_commitment() != head.custody_commitment()
            || head
                .assignment_continuation()
                .and_then(|value| value.allocation_range())
                != authority.allocation_range
        {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        Ok(DraftMarkerLabelAssignmentFlightV1 {
            state: AssignmentFlightState::Ready {
                owner,
                command,
                reservation,
                authority,
                retained_limits: super::DraftMarkerAdmissionLimitsV1::PRODUCTION,
                command_limit: AssignmentCommandLimit::Exact(
                    DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES,
                ),
            },
        })
    }

    pub fn submit_draft_marker_label_assignment(
        &self,
        store: &HomeStore,
        flight: DraftMarkerLabelAssignmentFlightV1,
    ) -> DraftMarkerLabelAssignmentOutcomeV1 {
        match flight.state {
            AssignmentFlightState::Ready {
                owner,
                command,
                reservation,
                authority,
                retained_limits,
                command_limit,
            } => self.submit_ready_assignment(
                store,
                owner,
                command,
                reservation,
                authority,
                retained_limits,
                command_limit,
            ),
            AssignmentFlightState::Reconciling {
                owner,
                handle,
                retry_failed,
                authority,
            } => {
                self.finish_assignment_reconciliation(store, owner, handle, retry_failed, authority)
            }
        }
    }

    fn submit_ready_assignment(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
        reservation: DraftMarkerAdmissionPreparedAttempt,
        authority: DraftMarkerAdmissionLiveAuthorityV1,
        retained_limits: super::DraftMarkerAdmissionLimitsV1,
        command_limit: AssignmentCommandLimit,
    ) -> DraftMarkerLabelAssignmentOutcomeV1 {
        let _reservation = reservation.disarm();
        match store.execute_current(self.handle.current_command(AssignmentMutation {
            owner,
            command,
            authority: authority.clone(),
            retained_limits,
            command_limit,
        })) {
            CommandOutcome::NotCommitted { evidence } => {
                let classification = classify_assignment_failure(&evidence);
                let terminal = matches!(
                    classification,
                    AssignmentFailureClass::Collision | AssignmentFailureClass::Rejected
                );
                if !self.finish_assignment_attempt(store, owner, command, true, terminal) {
                    return refused_unavailable();
                }
                match classification {
                    AssignmentFailureClass::Collision => {
                        DraftMarkerLabelAssignmentOutcomeV1::Collision
                    }
                    AssignmentFailureClass::Rejected => {
                        DraftMarkerLabelAssignmentOutcomeV1::Refused(
                            DraftMarkerLabelAssignmentRefusalV1::Rejected,
                        )
                    }
                    AssignmentFailureClass::Unavailable => refused_unavailable(),
                    AssignmentFailureClass::Retryable => {
                        DraftMarkerLabelAssignmentOutcomeV1::Retryable
                    }
                }
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                if let Some(local_finalization) = local_finalization {
                    if !matches!(
                        store.with_committed_local_finalization(
                            local_finalization,
                            &receipt,
                            &self.handle,
                            |attachment| {
                                attachment.finish_submission(owner, command, true, false, 0)
                            },
                        ),
                        Ok(Ok(()))
                    ) {
                        return refused_unavailable();
                    }
                    return refused_unavailable();
                }
                if !self.finish_assignment_attempt(store, owner, command, true, false) {
                    return refused_unavailable();
                }
                self.assignment_advanced_or_ready(store, owner, authority, receipt, later_failure)
            }
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                let _ = self.finish_assignment_attempt(store, owner, command, true, true);
                DraftMarkerLabelAssignmentOutcomeV1::ReconciliationPending(
                    DraftMarkerLabelAssignmentFlightV1 {
                        state: AssignmentFlightState::Reconciling {
                            owner,
                            handle: reconciliation.install_and_handle(),
                            retry_failed: false,
                            authority,
                        },
                    },
                )
            }
        }
    }

    fn finish_assignment_reconciliation(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        handle: ReconciliationHandle,
        retry_failed: bool,
        authority: DraftMarkerAdmissionLiveAuthorityV1,
    ) -> DraftMarkerLabelAssignmentOutcomeV1 {
        let resolution = if retry_failed {
            store.retry_reconciliation(&handle)
        } else {
            store.reconcile(&handle)
        };
        match resolution {
            Ok(ReconciliationResolution::ExactOld) => {
                if self.resolve_assignment_attempt(store, owner, true, false) {
                    DraftMarkerLabelAssignmentOutcomeV1::Retryable
                } else {
                    refused_unavailable()
                }
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                if !self.resolve_assignment_attempt(store, owner, true, false) {
                    return refused_unavailable();
                }
                self.assignment_advanced_or_ready(store, owner, authority, receipt, None)
            }
            Ok(ReconciliationResolution::Collision)
            | Ok(ReconciliationResolution::ExactSuccessor { .. }) => {
                let _ = self.resolve_assignment_attempt(store, owner, true, true);
                DraftMarkerLabelAssignmentOutcomeV1::Collision
            }
            Err(_) => DraftMarkerLabelAssignmentOutcomeV1::ReconciliationPending(
                DraftMarkerLabelAssignmentFlightV1 {
                    state: AssignmentFlightState::Reconciling {
                        owner,
                        handle,
                        retry_failed: true,
                        authority,
                    },
                },
            ),
        }
    }

    fn assignment_advanced_or_ready(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        authority: DraftMarkerAdmissionLiveAuthorityV1,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    ) -> DraftMarkerLabelAssignmentOutcomeV1 {
        match self.issue_draft_marker_label_readiness_proof(store, owner, authority) {
            Ok(Some(proof)) => DraftMarkerLabelAssignmentOutcomeV1::Ready {
                proof,
                receipt,
                later_failure,
            },
            Ok(None) => DraftMarkerLabelAssignmentOutcomeV1::Advanced {
                receipt,
                later_failure,
            },
            Err(_) => refused_unavailable(),
        }
    }

    fn issue_draft_marker_label_readiness_proof(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        authority: DraftMarkerAdmissionLiveAuthorityV1,
    ) -> Result<Option<DraftMarkerLabelReadinessProofV1>, DraftMarkerLabelAssignmentErrorV1> {
        let retained = store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.live_authority(owner)
            })
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?;
        if retained.authority != authority.authority
            || retained.allocation_range != authority.allocation_range
        {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        let generation = store
            .health()
            .generation()
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Unavailable)?;
        if generation.get() != authority.authority.home_generation.get() {
            return Err(DraftMarkerLabelAssignmentErrorV1::Unavailable);
        }
        let head = self
            .point::<DraftMarkerAdmissionHeadsFamily>(
                store,
                owner,
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        if head.lifecycle() == DraftMarkerAdmissionLifecycleV1::Assigning {
            return Ok(None);
        }
        let empty = canonical_empty_draft_marker_admission_root_v1(
            super::DraftMarkerAdmissionTreeV1::SourceOrder,
        );
        if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Ready
            || head.home_generation() != authority.authority.home_generation
            || head.owner() != owner
            || head.request_commitment() != authority.authority.request_commitment()
            || head.custody_commitment() != authority.authority.custody_commitment()
            || head.source_root() != empty
            || head.unassigned_count() != 0
            || head.target_root().count() != head.remaining_builder_count()
            || head.selected_receipt().is_none()
        {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        let selected = head
            .selected_receipt()
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        let replay = self
            .point::<DraftMarkerAdmissionReceiptsFamily>(
                store,
                DraftMarkerAdmissionReceiptKeyV1::new(owner, selected),
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        if replay.owner() != owner
            || replay.command_id() != selected
            || replay.transition() != DraftMarkerAdmissionReceiptTransitionV1::Assignment
            || replay.request_commitment() != head.request_commitment()
            || replay.source_after() != head.source_root()
            || replay.target_after() != head.target_root()
        {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        let session = self
            .point::<DraftEditorCandidateSessionsFamily>(
                store,
                DraftEditorCandidateSessionRecordKeyV1::head(
                    authority.authority.session.draft_id(),
                    authority.authority.session.session_id(),
                ),
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?;
        let label_authority = self
            .point::<ImageLabelAuthorityHeadsFamily>(
                store,
                authority.authority.session.thread_id(),
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?;
        let protection = self
            .point::<DraftImageLabelProtectionHeadsFamily>(
                store,
                authority.authority.session.thread_id(),
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?;
        if session
            != Some(DraftEditorCandidateSessionRecordV1::Head(
                authority.authority.session.clone(),
            ))
            || label_authority != Some(authority.authority.label_authority)
            || protection != Some(authority.authority.protection)
        {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        Ok(Some(DraftMarkerLabelReadinessProofV1 {
            home_generation: authority.authority.home_generation,
            owner,
            label_authority: authority.authority.label_authority,
            protection: authority.authority.protection,
            session: authority.authority.session,
            disposition: authority.authority.disposition,
            occurrence_commitment: head.occurrence_commitment(),
            assigned_target_root: head.target_root(),
            allocation_range: authority.allocation_range,
        }))
    }

    fn finish_assignment_attempt(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
        retain: bool,
        uncertain: bool,
    ) -> bool {
        matches!(
            store.with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.finish_submission(owner, command, retain, uncertain, 0)
            }),
            Ok(Ok(()))
        )
    }

    fn resolve_assignment_attempt(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        retain: bool,
        uncertain: bool,
    ) -> bool {
        matches!(
            store.with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.resolve_submission(owner, retain, uncertain, 0)
            }),
            Ok(Ok(()))
        )
    }
}

fn refused_unavailable() -> DraftMarkerLabelAssignmentOutcomeV1 {
    DraftMarkerLabelAssignmentOutcomeV1::Refused(DraftMarkerLabelAssignmentRefusalV1::Unavailable)
}
