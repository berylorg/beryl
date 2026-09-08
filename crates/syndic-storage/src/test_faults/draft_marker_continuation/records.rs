use super::*;
use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeCommand, MutationBuilder,
    ReconciliationReservation,
};

pub(super) fn write_endpoint(
    store: &HomeStore,
    storage: &SyndicStorage,
    build: DraftPieceBuildRecordV1,
    receipt: Option<DraftPieceBuildProgressReceiptV1>,
    remove_receipt: Option<DraftPieceBuildProgressReceiptKeyV1>,
    session: DraftEditorCandidateSessionRecordV1,
) {
    let contribution = storage.handle.contribution(
        storage.revision(store).unwrap(),
        MarkerEndpointReplacement {
            build,
            receipt,
            remove_receipt,
            session,
        },
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    let result = store.execute(command);
    assert!(
        matches!(
            result,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "fixture endpoint was not committed: {result:?}"
    );
}

struct MarkerEndpointReplacement {
    build: DraftPieceBuildRecordV1,
    receipt: Option<DraftPieceBuildProgressReceiptV1>,
    remove_receipt: Option<DraftPieceBuildProgressReceiptKeyV1>,
    session: DraftEditorCandidateSessionRecordV1,
}

impl DomainMutation<SyndicDomain> for MarkerEndpointReplacement {
    type Error = crate::SyndicMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        if self.receipt.is_some() || self.remove_receipt.is_some() {
            reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let key = DraftPieceSettlementKeyV1::new(
            prepared.build.draft_id(),
            prepared.build.session_id(),
            prepared.build.operation_id(),
        );
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            prepared.build.draft_id(),
            prepared.build.session_id(),
        );
        builder.put::<DraftPieceBuildsCodec>(&key, &prepared.build)?;
        if let Some(receipt) = prepared.receipt {
            builder.put::<DraftPieceBuildProgressCodec>(&receipt.key(), &receipt)?;
        }
        if let Some(key) = prepared.remove_receipt {
            builder.delete::<DraftPieceBuildProgressCodec>(&key)?;
        }
        builder.put::<DraftEditorCandidateSessionsCodec>(&session_key, &prepared.session)?;
        Ok(())
    }
}
