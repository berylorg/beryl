use super::*;
use crate::draft_piece::mutation::advance_budget::{BuildAcquisition, BuildBudget};

#[derive(Clone)]
pub(super) enum IndexWork {
    Admission(AdmissionWorkLedger),
    Builder(BuildBudget),
}

impl From<AdmissionWorkLedger> for IndexWork {
    fn from(value: AdmissionWorkLedger) -> Self {
        Self::Admission(value)
    }
}

impl IndexWork {
    pub(super) fn reserve_node_point(
        &self,
        bytes: u64,
    ) -> Result<Option<super::super::ledger::PointReservation>, DraftMarkerAdmissionSchemaErrorV1>
    {
        match self {
            Self::Admission(work) => work.reserve_node_point(bytes).map(Some),
            Self::Builder(_) => Ok(None),
        }
    }

    pub(super) fn charge_emit(
        &self,
        bytes: u64,
        structure: bool,
    ) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        match self {
            Self::Admission(work) => work.charge_emit(bytes, structure),
            Self::Builder(work) => work
                .encoded_effect(bytes, structure)
                .map_err(|_| DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge),
        }
    }

    pub(super) fn charge_delete(
        &self,
        bytes: u64,
    ) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        match self {
            Self::Admission(work) => work.charge_delete(bytes),
            Self::Builder(_) => Err(DraftMarkerAdmissionSchemaErrorV1::InvalidRoot),
        }
    }

    pub(super) fn charge_node_delete(
        &self,
        node: &DraftMarkerAdmissionNodeV1,
    ) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
        let bytes = match self {
            Self::Admission(work) => {
                let bytes = encoded_node_record_charge(&node.key(), node)?;
                work.charge_delete(bytes)?;
                bytes
            }
            Self::Builder(work) => {
                let bytes = encoded_node_key_charge(&node.key())?;
                work.encoded_effect(bytes, false)
                    .map_err(|_| DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge)?;
                bytes
            }
        };
        Ok(bytes)
    }
}

impl AdmissionNodeReader for BuildAcquisition<'_> {
    fn point(
        &self,
        key: &DraftMarkerAdmissionNodeKeyV1,
    ) -> Result<Option<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionIndexPreparationErrorV1>
    {
        self.point::<super::super::DraftMarkerAdmissionNodesFamily>(*key)
            .map_err(|error| match error {
                crate::draft_piece::DraftPiecePrepareErrorV1::Read(error) => {
                    DraftMarkerAdmissionIndexPreparationErrorV1::StoreRead(error)
                }
                crate::draft_piece::DraftPiecePrepareErrorV1::Rejected(_) => {
                    DraftMarkerAdmissionIndexPreparationErrorV1::OperationTooLarge
                }
                _ => DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication,
            })
    }
}

pub(crate) fn prepare_acquired_marker_consumption(
    acquisition: &BuildAcquisition<'_>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
    marker: DraftPieceMarkerV1,
    identity: DraftMarkerAdmissionPageIdentityV1,
) -> Result<PreparedDraftMarkerAdmissionConsumptionV1, DraftMarkerAdmissionIndexPreparationErrorV1>
{
    prepare_consumption_with_work(
        acquisition,
        owner,
        root,
        marker,
        identity,
        IndexWork::Builder(acquisition.budget.clone()),
    )
}
