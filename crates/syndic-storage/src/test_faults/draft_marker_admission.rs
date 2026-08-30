use std::num::NonZeroU64;

use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation, RecordCodec,
};
use beryl_model::DomainRevision;

use crate::{
    SyndicStorage,
    codec::{ExactCodec, Family},
    domain::SyndicDomain,
    draft_piece::{
        DraftMarkerAdmissionCapacityFamily, DraftMarkerAdmissionCapacityKeyV1,
        DraftMarkerAdmissionCapacityV1, DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionHeadsCodec,
        DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionNodesCodec,
        DraftMarkerAdmissionReceiptKeyV1, DraftMarkerAdmissionReceiptsCodec,
        DraftMarkerAdmissionReplayReceiptV1, DraftMarkerAdmissionRetainedChargeV1,
    },
};

use super::FixtureMutationError;

pub fn inject_malformed_draft_marker_admission_capacity(
    store: &HomeStore,
    storage: SyndicStorage,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    inject(
        store,
        storage,
        DraftMarkerAdmissionCapacityFamily::RECORD_VERSION
            .get()
            .to_be_bytes()
            .to_vec(),
    )
}

#[derive(Clone)]
struct CapacityWithoutHeads(DraftMarkerAdmissionCapacityV1);

impl DomainMutation<SyndicDomain> for CapacityWithoutHeads {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ExactCodec<DraftMarkerAdmissionCapacityFamily>>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        builder.put::<ExactCodec<DraftMarkerAdmissionCapacityFamily>>(
            &DraftMarkerAdmissionCapacityKeyV1,
            &prepared.0,
        )?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct DraftMarkerAdmissionFixtureSnapshotV1 {
    capacity: DraftMarkerAdmissionCapacityV1,
    heads: Box<[DraftMarkerAdmissionHeadV1]>,
    nodes: Box<[DraftMarkerAdmissionNodeV1]>,
    receipts: Box<[DraftMarkerAdmissionReplayReceiptV1]>,
}

impl DraftMarkerAdmissionFixtureSnapshotV1 {
    pub fn new(
        capacity: DraftMarkerAdmissionCapacityV1,
        heads: impl Into<Box<[DraftMarkerAdmissionHeadV1]>>,
        nodes: impl Into<Box<[DraftMarkerAdmissionNodeV1]>>,
        receipts: impl Into<Box<[DraftMarkerAdmissionReplayReceiptV1]>>,
    ) -> Self {
        Self {
            capacity,
            heads: heads.into(),
            nodes: nodes.into(),
            receipts: receipts.into(),
        }
    }
}

impl DomainMutation<SyndicDomain> for DraftMarkerAdmissionFixtureSnapshotV1 {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ExactCodec<DraftMarkerAdmissionCapacityFamily>>(1)?;
        if !self.heads.is_empty() {
            reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(self.heads.len())?;
        }
        if !self.nodes.is_empty() {
            reservation.reserve_records::<DraftMarkerAdmissionNodesCodec>(self.nodes.len())?;
        }
        if !self.receipts.is_empty() {
            reservation
                .reserve_records::<DraftMarkerAdmissionReceiptsCodec>(self.receipts.len())?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        builder.put::<ExactCodec<DraftMarkerAdmissionCapacityFamily>>(
            &DraftMarkerAdmissionCapacityKeyV1,
            &prepared.capacity,
        )?;
        for head in prepared.heads.iter() {
            builder.put::<DraftMarkerAdmissionHeadsCodec>(&head.owner(), head)?;
        }
        for node in prepared.nodes.iter() {
            builder.put::<DraftMarkerAdmissionNodesCodec>(&node.key(), node)?;
        }
        for receipt in prepared.receipts.iter() {
            builder.put::<DraftMarkerAdmissionReceiptsCodec>(
                &DraftMarkerAdmissionReceiptKeyV1::new(receipt.owner(), receipt.command_id()),
                receipt,
            )?;
        }
        Ok(())
    }
}

pub fn draft_marker_admission_fixture_contribution(
    storage: &SyndicStorage,
    expected_revision: DomainRevision,
    snapshot: DraftMarkerAdmissionFixtureSnapshotV1,
) -> MutationContribution {
    storage.handle.contribution(expected_revision, snapshot)
}

pub fn draft_marker_admission_capacity_without_heads_contribution(
    storage: &SyndicStorage,
    expected_revision: DomainRevision,
) -> MutationContribution {
    let capacity = DraftMarkerAdmissionCapacityV1::new(
        NonZeroU64::MIN,
        DraftMarkerAdmissionRetainedChargeV1::new(1, 0, 1),
    )
    .expect("bounded semantic-corruption fixture is codec-valid");
    storage
        .handle
        .contribution(expected_revision, CapacityWithoutHeads(capacity))
}

fn inject(
    store: &HomeStore,
    storage: SyndicStorage,
    encoded_value: Vec<u8>,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let encoded_key = <ExactCodec<DraftMarkerAdmissionCapacityFamily> as RecordCodec<
        SyndicDomain,
    >>::encode_key(&DraftMarkerAdmissionCapacityKeyV1)
    .expect("singleton admission-capacity key encodes");
    store.inject_persisted_corrupt_record::<
        SyndicDomain,
        ExactCodec<DraftMarkerAdmissionCapacityFamily>,
    >(&storage.handle, &encoded_key, &encoded_value)
}
