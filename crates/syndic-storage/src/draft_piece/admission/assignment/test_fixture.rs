use beryl_home_store::HomeStore;
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftMarkerId, SyndicThreadId};

use crate::{
    DraftMarkerAdmissionLimitsV1, SyndicStorage,
    draft_piece::{
        DraftMarkerAdmissionNodePayloadV1, DraftMarkerAdmissionNodesFamily,
        DraftMarkerAdmissionTargetDispositionV1,
    },
};

use super::{
    AssignmentCommandLimit, AssignmentFlightState, DraftMarkerAdmissionCommandIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerLabelAssignmentErrorV1,
    DraftMarkerLabelAssignmentFlightV1, DraftMarkerLabelReadinessProofV1,
};

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
