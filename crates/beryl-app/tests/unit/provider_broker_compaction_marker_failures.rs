use super::*;
use beryl_backend::{OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause};

impl Fixture {
    fn begin_context_compaction_marker(&mut self) {
        self.submit_applied(OrderedTurnStreamOperation::ProviderBegin(
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Started,
                kind: ProviderItemKind::ContextCompaction,
            },
        ));
    }

    fn stage_marker_timestamp_and_open_identity(&mut self, observed_at: u64) {
        self.submit_applied(OrderedTurnStreamOperation::ProviderControl(
            ProviderObservationControl::Scalar {
                context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
                value: ProviderScalar::Unsigned(observed_at),
            },
        ));
        self.submit_applied(OrderedTurnStreamOperation::ProviderControl(
            ProviderObservationControl::BeginField(ProviderValueContext::Field(
                ProviderField::ItemId,
            )),
        ));
    }

    fn assert_schema_rejection(&mut self, operation: OrderedTurnStreamOperation) {
        assert!(matches!(
            self.sink.as_mut().unwrap().submit(operation),
            Err(error)
                if error.cause()
                    == OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::SchemaMismatch
                    )
        ));
    }
}

#[test]
fn resident_marker_rejects_incomplete_schema_without_a_staging_callback() {
    let mut fixture = Fixture::new(183);
    fixture.begin_context_compaction_marker();
    fixture.assert_schema_rejection(OrderedTurnStreamOperation::ProviderSeal(
        ProviderObservationRoute::new(
            fixture.cas_thread_id.clone(),
            fixture.cas_turn_id.clone(),
        ),
    ));

    #[cfg(feature = "test-faults")]
    assert_eq!(
        fixture
            .broker
            .as_ref()
            .unwrap()
            .test_snapshot()
            .provider_staging_batches(),
        0
    );
    assert_eq!(
        fixture.operation().provider_frontier(),
        Some(CompactionProviderSequence::new(2).unwrap())
    );
}

#[test]
fn resident_marker_rejects_identity_beyond_256_bytes_without_a_staging_callback() {
    let mut fixture = Fixture::new(184);
    fixture.begin_context_compaction_marker();
    fixture.stage_marker_timestamp_and_open_identity(194);
    let oversized = [b'x'; 257];
    let context = ProviderValueContext::Field(ProviderField::ItemId);
    let mut page = match fixture
        .sink
        .as_mut()
        .unwrap()
        .submit(OrderedTurnStreamOperation::ProviderAcquirePage)
        .unwrap()
    {
        OrderedTurnStreamCompletion::PageLease(page) => page,
        completion => panic!("unexpected acquire completion: {completion:?}"),
    };
    page.buffer_mut()[..oversized.len()].copy_from_slice(&oversized);
    page.set_len(oversized.len()).unwrap();
    fixture.assert_schema_rejection(OrderedTurnStreamOperation::ProviderFragment(
        provider_observation_fragment(context, page),
    ));

    #[cfg(feature = "test-faults")]
    assert_eq!(
        fixture
            .broker
            .as_ref()
            .unwrap()
            .test_snapshot()
            .provider_staging_batches(),
        0
    );
}

#[test]
fn resident_marker_with_unmatched_route_publishes_nothing() {
    let mut fixture = Fixture::new(185);
    fixture.begin_context_compaction_marker();
    fixture.stage_marker_timestamp_and_open_identity(195);
    let context = ProviderValueContext::Field(ProviderField::ItemId);
    fixture.submit_fragment(context, b"unmatched-marker");
    fixture.submit_applied(OrderedTurnStreamOperation::ProviderControl(
        ProviderObservationControl::EndField(context),
    ));
    fixture.submit_applied(OrderedTurnStreamOperation::ProviderSeal(
        ProviderObservationRoute::new(
            fixture.cas_thread_id.clone(),
            beryl_model::CasTurnId::new("unmatched-marker-turn").unwrap(),
        ),
    ));

    assert_eq!(
        fixture.operation().provider_frontier(),
        Some(CompactionProviderSequence::new(2).unwrap())
    );
    #[cfg(feature = "test-faults")]
    assert_eq!(
        fixture
            .broker
            .as_ref()
            .unwrap()
            .test_snapshot()
            .provider_staging_batches(),
        0
    );
}
