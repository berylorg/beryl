use std::num::NonZeroU64;

use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeStore, MutationBuilder,
    ReconciliationReservation, ReconciliationResolution,
};
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftMarkerId, SyndicThreadId};

use crate::{
    DraftEditorCandidateSessionV1, DraftMarkerAdmissionLimitsV1, DraftPieceMarkerV1,
    SyndicMutationError, SyndicStorage,
    codec::{DraftImageLabelProtectionHeadsFamily, ImageLabelAuthorityHeadsFamily},
    domain::SyndicDomain,
    draft_piece::{
        DraftEditorCandidateSessionRecordKeyV1, DraftEditorCandidateSessionRecordV1,
        DraftEditorCandidateSessionsFamily, DraftMarkerAdmissionCapacityCodec,
        DraftMarkerAdmissionCapacityFamily, DraftMarkerAdmissionCapacityKeyV1,
        DraftMarkerAdmissionCapacityV1, DraftMarkerAdmissionEvidenceV1, DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
        DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodeIdV1,
        DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionNodeKindV1,
        DraftMarkerAdmissionNodePayloadV1, DraftMarkerAdmissionNodeV1,
        DraftMarkerAdmissionNodesCodec, DraftMarkerAdmissionNodesFamily,
        DraftMarkerAdmissionPageIdentityV1, DraftMarkerAdmissionReceiptKeyV1,
        DraftMarkerAdmissionReceiptTransitionV1, DraftMarkerAdmissionReceiptsCodec,
        DraftMarkerAdmissionReceiptsFamily, DraftMarkerAdmissionReplayReceiptV1,
        DraftMarkerAdmissionRetainedChargeV1, DraftMarkerAdmissionRootV1,
        DraftMarkerAdmissionTargetDispositionV1, DraftMarkerAdmissionTreeV1,
        DraftMarkerLabelReadinessDispositionV1, DraftMarkerLabelReadinessRequestAuthorityV1,
        canonical_empty_draft_marker_admission_root_v1,
        checked_draft_marker_admission_command_charge_v1, encoded_capacity_record_charge,
        encoded_head_record_charge, encoded_node_record_charge, encoded_receipt_record_charge,
    },
    mutation::{point, required},
};

use super::{
    AssignmentCommandLimit, AssignmentFlightState, DraftMarkerAdmissionCommandIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerLabelAssignmentErrorV1,
    DraftMarkerLabelAssignmentFlightV1, DraftMarkerLabelReadinessProofV1,
};

#[derive(Clone)]
struct ReadyTargetFixtureMutation {
    session: DraftEditorCandidateSessionV1,
    authority: DraftMarkerLabelReadinessRequestAuthorityV1,
    node: DraftMarkerAdmissionNodeV1,
    head: DraftMarkerAdmissionHeadV1,
    receipt: DraftMarkerAdmissionReplayReceiptV1,
}

struct PreparedReadyTargetFixtureMutation {
    node: DraftMarkerAdmissionNodeV1,
    head: DraftMarkerAdmissionHeadV1,
    receipt: DraftMarkerAdmissionReplayReceiptV1,
    capacity: DraftMarkerAdmissionCapacityV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAssignedAssociationV1 {
    target_marker_id: SyndicDraftMarkerId,
    assigned_label: ImageLabelOrdinal,
    asset_id: AssetId,
}

impl DraftMarkerAssignedAssociationV1 {
    pub const fn target_marker_id(self) -> SyndicDraftMarkerId {
        self.target_marker_id
    }

    pub const fn assigned_label(self) -> ImageLabelOrdinal {
        self.assigned_label
    }

    pub const fn asset_id(self) -> AssetId {
        self.asset_id
    }
}

impl SyndicStorage {
    pub fn seed_draft_marker_writer_ready_target_for_test(
        &self,
        store: &HomeStore,
        session: &DraftEditorCandidateSessionV1,
        owner: DraftMarkerAdmissionOwnerV1,
        marker: DraftPieceMarkerV1,
    ) -> Result<DraftMarkerLabelReadinessProofV1, DraftMarkerLabelAssignmentErrorV1> {
        if owner.draft_id() != session.draft_id() || owner.session_id() != session.session_id() {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        let generation = store
            .health()
            .generation()
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Unavailable)?;
        let home_generation = NonZeroU64::new(generation.get())
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Unavailable)?;
        let label_authority = self
            .point::<ImageLabelAuthorityHeadsFamily>(
                store,
                session.thread_id(),
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        let protection = self
            .point::<DraftImageLabelProtectionHeadsFamily>(
                store,
                session.thread_id(),
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?
            .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        let authority = DraftMarkerLabelReadinessRequestAuthorityV1 {
            home_generation,
            label_authority,
            protection,
            session: session.clone(),
            disposition: DraftMarkerLabelReadinessDispositionV1::Reuse,
        };
        let command = DraftMarkerAdmissionCommandIdV1::from_bytes(*owner.operation_id().as_bytes());
        let prepared_attempt = store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.prepare_attempt(owner, command, 0, &authority, None)
            })
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        if prepared_attempt.was_present() {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        let (node, head, receipt) =
            ready_target_fixture_records(owner, marker, &authority, command)?;
        let assigned_target_root = head.target_root();
        let outcome =
            store.execute_current(self.handle.current_command(ReadyTargetFixtureMutation {
                session: session.clone(),
                authority: authority.clone(),
                node,
                head,
                receipt,
            }));
        let retained = match outcome {
            CommandOutcome::Committed { .. } => {
                let _reservation = prepared_attempt
                    .disarm()
                    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?;
                true
            }
            CommandOutcome::NotCommitted { .. } => {
                return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
            }
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                let _reservation = prepared_attempt
                    .disarm()
                    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?;
                match store
                    .reconcile(&reconciliation.install_and_handle())
                    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?
                {
                    ReconciliationResolution::ExactNew { .. } => true,
                    ReconciliationResolution::ExactOld
                    | ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision => false,
                }
            }
        };
        store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.finish_submission(owner, command, retained, false, 0)
            })
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?;
        if !retained {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        Ok(DraftMarkerLabelReadinessProofV1 {
            home_generation,
            owner,
            label_authority,
            protection,
            session: session.clone(),
            disposition: DraftMarkerLabelReadinessDispositionV1::Reuse,
            occurrence_commitment: authority.occurrence_commitment(),
            assigned_target_root,
            allocation_range: None,
        })
    }

    pub fn prepare_draft_marker_label_assignment_with_limits_for_test(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
        retained_limits: DraftMarkerAdmissionLimitsV1,
        command_limit: u64,
    ) -> Result<DraftMarkerLabelAssignmentFlightV1, DraftMarkerLabelAssignmentErrorV1> {
        let mut flight = self.prepare_draft_marker_label_assignment(store, owner, command)?;
        let AssignmentFlightState::Ready {
            retained_limits: flight_retained_limits,
            command_limit: flight_command_limit,
            ..
        } = &mut flight.state
        else {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        };
        *flight_retained_limits = retained_limits;
        *flight_command_limit = AssignmentCommandLimit::Exact(command_limit);
        Ok(flight)
    }

    pub fn prepare_draft_marker_label_assignment_at_pre_authority_read_ceiling_for_test(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
    ) -> Result<DraftMarkerLabelAssignmentFlightV1, DraftMarkerLabelAssignmentErrorV1> {
        let mut flight = self.prepare_draft_marker_label_assignment(store, owner, command)?;
        let AssignmentFlightState::Ready { command_limit, .. } = &mut flight.state else {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        };
        *command_limit = AssignmentCommandLimit::BeforeAuthorityReads;
        Ok(flight)
    }

    pub fn seed_draft_marker_label_allocation_frontier_for_test(
        &self,
        store: &HomeStore,
        destination: SyndicThreadId,
        frontier: ImageLabelOrdinal,
    ) -> Result<(), DraftMarkerLabelAssignmentErrorV1> {
        store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.seed_allocation_frontier_for_test(destination, frontier)
            })
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)?
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Unavailable)
    }

    pub fn inspect_draft_marker_label_readiness_proof_for_test(
        &self,
        store: &HomeStore,
        proof: &DraftMarkerLabelReadinessProofV1,
    ) -> Result<Box<[DraftMarkerAssignedAssociationV1]>, DraftMarkerLabelAssignmentErrorV1> {
        if store
            .health()
            .generation()
            .is_none_or(|generation| generation.get() != proof.home_generation().get())
        {
            return Err(DraftMarkerLabelAssignmentErrorV1::Unavailable);
        }
        let mut associations = Vec::new();
        let mut pending = proof
            .assigned_target_root()
            .node()
            .into_iter()
            .collect::<Vec<_>>();
        while let Some(key) = pending.pop() {
            let node = self
                .point::<DraftMarkerAdmissionNodesFamily>(
                    store,
                    key,
                    crate::draft_piece::point_limit(),
                )
                .map_err(DraftMarkerLabelAssignmentErrorV1::Read)?
                .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
            if node.key() != key || node.key().owner() != proof.owner() {
                return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
            }
            match node.payload() {
                DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } => {
                    pending.extend(children.iter().rev().map(|child| child.key()));
                }
                DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                    target_marker_id,
                    asset_id,
                    disposition: DraftMarkerAdmissionTargetDispositionV1::Assigned(label),
                    ..
                } => associations.push(DraftMarkerAssignedAssociationV1 {
                    target_marker_id: *target_marker_id,
                    assigned_label: *label,
                    asset_id: *asset_id,
                }),
                _ => return Err(DraftMarkerLabelAssignmentErrorV1::Rejected),
            }
        }
        associations.sort_by_key(|association| association.target_marker_id);
        if associations.len() as u64 != proof.assigned_target_root().count() {
            return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
        }
        Ok(associations.into_boxed_slice())
    }
}

fn ready_target_fixture_records(
    owner: DraftMarkerAdmissionOwnerV1,
    marker: DraftPieceMarkerV1,
    authority: &DraftMarkerLabelReadinessRequestAuthorityV1,
    command: DraftMarkerAdmissionCommandIdV1,
) -> Result<
    (
        DraftMarkerAdmissionNodeV1,
        DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionReplayReceiptV1,
    ),
    DraftMarkerLabelAssignmentErrorV1,
> {
    let empty_source =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let empty_target =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    let node_key = DraftMarkerAdmissionNodeKeyV1::new(
        owner,
        DraftMarkerAdmissionNodeKindV1::Leaf,
        DraftMarkerAdmissionNodeIdV1::from_bytes(*marker.marker_id().as_bytes()),
    );
    let page = DraftMarkerAdmissionPageIdentityV1::new(command, NonZeroU64::MIN);
    let evidence = DraftMarkerAdmissionEvidenceV1::new(b"phase225-ready-target-fixture".as_slice())
        .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let node = DraftMarkerAdmissionNodeV1::target_leaf(
        node_key,
        marker.marker_id(),
        page,
        evidence,
        marker.label(),
        marker.asset_id(),
        DraftMarkerAdmissionTargetDispositionV1::Assigned(marker.label()),
    )
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let target_root = DraftMarkerAdmissionRootV1::new(
        DraftMarkerAdmissionTreeV1::TargetId,
        node_key,
        1,
        node.digest(),
        1,
    )
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        owner,
        command,
        NonZeroU64::MIN,
        authority.request_commitment(),
        b"phase225-ready-source".as_slice(),
        b"phase225-ready-target".as_slice(),
        empty_source,
        empty_source,
        empty_target,
        target_root,
        Box::default(),
        DraftMarkerAdmissionReceiptTransitionV1::Assignment,
    )
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let make_head = |charge| {
        DraftMarkerAdmissionHeadV1::new(
            owner,
            NonZeroU64::MIN,
            authority.home_generation,
            DraftMarkerAdmissionLifecycleV1::Ready,
            authority.request_commitment(),
            authority.custody_commitment(),
            NonZeroU64::MIN,
            0,
            true,
            Some(command),
            empty_source,
            target_root,
            authority.occurrence_commitment(),
            0,
            None,
            1,
            charge,
            None,
        )
    };
    let provisional = make_head(DraftMarkerAdmissionRetainedChargeV1::new(1, 1, 0))
        .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(owner, command);
    let retained_bytes = encoded_node_record_charge(&node_key, &node)
        .and_then(|bytes| {
            bytes
                .checked_add(encoded_head_record_charge(&owner, &provisional)?)
                .ok_or(super::DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
        })
        .and_then(|bytes| {
            bytes
                .checked_add(encoded_receipt_record_charge(&receipt_key, &receipt)?)
                .ok_or(super::DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
        })
        .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let head = make_head(DraftMarkerAdmissionRetainedChargeV1::new(
        1,
        1,
        retained_bytes,
    ))
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    Ok((node, head, receipt))
}

impl DomainMutation<SyndicDomain> for ReadyTargetFixtureMutation {
    type Error = SyndicMutationError;
    type Prepared = PreparedReadyTargetFixtureMutation;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            self.session.draft_id(),
            self.session.session_id(),
        );
        if required::<DraftEditorCandidateSessionsFamily>(reader, &session_key)?
            != DraftEditorCandidateSessionRecordV1::Head(self.session)
            || required::<ImageLabelAuthorityHeadsFamily>(
                reader,
                &self.authority.label_authority.thread_id(),
            )? != self.authority.label_authority
            || required::<DraftImageLabelProtectionHeadsFamily>(
                reader,
                &self.authority.protection.thread_id(),
            )? != self.authority.protection
            || point::<DraftMarkerAdmissionHeadsFamily>(reader, &self.head.owner())?.is_some()
            || point::<DraftMarkerAdmissionNodesFamily>(reader, &self.node.key())?.is_some()
            || point::<DraftMarkerAdmissionReceiptsFamily>(
                reader,
                &DraftMarkerAdmissionReceiptKeyV1::new(
                    self.receipt.owner(),
                    self.receipt.command_id(),
                ),
            )?
            .is_some()
            || point::<DraftMarkerAdmissionCapacityFamily>(
                reader,
                &DraftMarkerAdmissionCapacityKeyV1,
            )?
            .is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let capacity = DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, self.head.charge())
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
        let write_bytes = encoded_node_record_charge(&self.node.key(), &self.node)
            .and_then(|bytes| {
                bytes
                    .checked_add(encoded_head_record_charge(&self.head.owner(), &self.head)?)
                    .ok_or(super::DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
            })
            .and_then(|bytes| {
                bytes
                    .checked_add(encoded_receipt_record_charge(
                        &DraftMarkerAdmissionReceiptKeyV1::new(
                            self.receipt.owner(),
                            self.receipt.command_id(),
                        ),
                        &self.receipt,
                    )?)
                    .ok_or(super::DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
            })
            .and_then(|bytes| {
                bytes
                    .checked_add(encoded_capacity_record_charge(
                        &DraftMarkerAdmissionCapacityKeyV1,
                        &capacity,
                    )?)
                    .ok_or(super::DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
            })
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
        checked_draft_marker_admission_command_charge_v1([0, write_bytes, 0])
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
        Ok(PreparedReadyTargetFixtureMutation {
            node: self.node,
            head: self.head,
            receipt: self.receipt,
            capacity,
        })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerAdmissionNodesCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<DraftMarkerAdmissionNodesCodec>(&prepared.node.key(), &prepared.node)?;
        mutations.put::<DraftMarkerAdmissionHeadsCodec>(&prepared.head.owner(), &prepared.head)?;
        mutations.put::<DraftMarkerAdmissionReceiptsCodec>(
            &DraftMarkerAdmissionReceiptKeyV1::new(
                prepared.receipt.owner(),
                prepared.receipt.command_id(),
            ),
            &prepared.receipt,
        )?;
        mutations.put::<DraftMarkerAdmissionCapacityCodec>(
            &DraftMarkerAdmissionCapacityKeyV1,
            &prepared.capacity,
        )?;
        Ok(())
    }
}
