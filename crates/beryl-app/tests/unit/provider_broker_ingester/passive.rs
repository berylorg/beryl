use super::*;
use crate::cas_projection::{
    ConnectionWorkTargetIdentity,
    outage_buffer::{
        OutageAssemblyLimits, OutageBufferLimits, OutageInventory, OutageInventoryAccess,
        OutageInventoryError, OutageTarget,
    },
    persistent_failure::{
        PersistentFailureCutIdentity, PersistentFailureGeneration, ProjectionServiceGeneration,
    },
};
use beryl_backend::{
    ApprovalOperationCompletion, ApprovalRequestKind, ApprovalResponseDisposition,
    OrderedTurnStreamCompletion, ProviderField, ProviderItemKind, ProviderItemLifecycle,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationRoute,
    ProviderValueContext, SteeringUserMessageCaptureMode,
    lifecycle_test_support::{approval_request, provider_observation_fragment},
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasThreadId, CasTurnId, SyndicThreadId,
    SyndicTurnId,
};

fn identity(fixture: &BrokerBuildFixture) -> PersistentFailureCutIdentity {
    PersistentFailureCutIdentity::new(
        fixture.home_id,
        fixture.home_generation,
        fixture.commands.service_generation(),
        PersistentFailureGeneration::FIRST,
    )
}

fn inventory(fixture: &BrokerBuildFixture, max_targets: usize) -> Arc<OutageInventory> {
    Arc::new(OutageInventory::new(
        identity(fixture),
        OutageBufferLimits {
            max_facts: 100,
            max_encoded_bytes: 65536,
            max_field_bytes: 512,
            max_targets,
        },
        OutageAssemblyLimits {
            max_bytes: 4096,
            max_field_bytes: 256,
            max_entries: 16,
        },
    ))
}

fn target(fixture: &BrokerBuildFixture, seed: u8) -> OutageTarget {
    let connection = fixture
        .authority
        .outage_identity(fixture.home_generation.get());
    OutageTarget::new(
        ConnectionWorkTargetIdentity {
            runtime_id: connection.runtime,
            process_generation: connection.process,
            connection_generation: connection.connection,
            registration_serial: u64::from(seed),
            thread_id: SyndicThreadId::from_bytes([seed; 16]),
            cas_thread_id: route(seed).thread_id().clone(),
            loaded_generation: CasLoadedSessionGeneration::new(
                connection.process,
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            home_generation: connection.home_generation,
        },
        SyndicTurnId::from_bytes([seed; 16]),
        route(seed).turn_id().clone(),
    )
}

fn route(seed: u8) -> ProviderObservationRoute {
    ProviderObservationRoute::new(
        CasThreadId::new(format!("thread-{seed}")).unwrap(),
        CasTurnId::new(format!("turn-{seed}")).unwrap(),
    )
}

fn begin(sink: &mut dyn OrderedTurnStreamSink) {
    assert!(matches!(
        sink.submit(OrderedTurnStreamOperation::ProviderBegin(
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::AgentMessage,
            }
        ))
        .unwrap(),
        OrderedTurnStreamCompletion::Applied
    ));
}

fn field(sink: &mut dyn OrderedTurnStreamSink, field: ProviderField, text: &str) {
    let context = ProviderValueContext::Field(field);
    sink.submit(OrderedTurnStreamOperation::ProviderControl(
        ProviderObservationControl::BeginField(context),
    ))
    .unwrap();
    let OrderedTurnStreamCompletion::PageLease(mut page) = sink
        .submit(OrderedTurnStreamOperation::ProviderAcquirePage)
        .unwrap()
    else {
        panic!("missing page")
    };
    page.buffer_mut()[..text.len()].copy_from_slice(text.as_bytes());
    page.set_len(text.len()).unwrap();
    let result = sink
        .submit(OrderedTurnStreamOperation::ProviderFragment(
            provider_observation_fragment(context, page),
        ))
        .unwrap();
    let OrderedTurnStreamCompletion::PageLease(page) = result else {
        panic!("missing returned page")
    };
    drop(page);
    sink.submit(OrderedTurnStreamOperation::ProviderControl(
        ProviderObservationControl::EndField(context),
    ))
    .unwrap();
}

fn complete(sink: &mut dyn OrderedTurnStreamSink, seed: u8) {
    begin(sink);
    field(sink, ProviderField::ItemId, "item");
    field(sink, ProviderField::AgentMessageText, "Hello 世界");
    sink.submit(OrderedTurnStreamOperation::ProviderSeal(route(seed)))
        .unwrap();
}

fn fail_during_begin(fixture: &BrokerBuildFixture, sink: &mut dyn OrderedTurnStreamSink) {
    fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::AfterPersist);
    begin(sink);
    assert_eq!(
        fixture.home.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    assert_eq!(
        sink.steering_user_message_capture_mode().unwrap(),
        SteeringUserMessageCaptureMode::Passive
    );
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
}

#[test]
fn pending_inventory_never_blocks_ack_and_idle_wake_flushes_latest_seal() {
    let fixture = BrokerBuildFixture::new(204);
    let prepared = fixture.prepare_start_blocked();
    let inventory = inventory(&fixture, 2);
    prepared
        .bind_outage_inventory(Arc::clone(&inventory))
        .unwrap();
    let PreparedProviderBroker {
        mut sink,
        control,
        ingester,
        start,
    } = prepared;
    let worker = ingester.start(start);
    fail_during_begin(&fixture, sink.as_mut());
    sink.submit(OrderedTurnStreamOperation::ProviderSeal(route(1)))
        .unwrap();
    for _ in 0..20 {
        complete(sink.as_mut(), 1);
    }
    let target = target(&fixture, 1);
    inventory
        .publish(identity(&fixture), &[target.clone()])
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let flushed = inventory.access(identity(&fixture), |access| match access {
            OutageInventoryAccess::Ready(buffer) => {
                let retained = buffer.retained().count() > 0;
                if retained {
                    assert!(buffer.has_gap(&target).unwrap());
                }
                retained
            }
            _ => panic!("inventory not ready"),
        });
        if flushed {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(control.passive_ready());
    drop(worker.stop_and_join().into_worker());
    assert!(!control.passive_ready());
    drop(sink);
    drop(control);
    inventory.retire();
    assert_eq!(fixture.workers.diagnostics().active(), 0);
}

#[test]
fn spanning_failure_marks_only_the_late_exact_route_and_approval_stays_unanswered() {
    let fixture = BrokerBuildFixture::new(205);
    let prepared = fixture.prepare_start_blocked();
    let inventory = inventory(&fixture, 2);
    let target = target(&fixture, 1);
    let sibling = self::target(&fixture, 2);
    inventory
        .publish(identity(&fixture), &[target.clone(), sibling.clone()])
        .unwrap();
    prepared
        .bind_outage_inventory(Arc::clone(&inventory))
        .unwrap();
    let PreparedProviderBroker {
        mut sink,
        control,
        ingester,
        start,
    } = prepared;
    let worker = ingester.start(start);
    fail_during_begin(&fixture, sink.as_mut());
    field(
        sink.as_mut(),
        ProviderField::ItemId,
        "discarded-whole-observation",
    );
    sink.submit(OrderedTurnStreamOperation::ProviderSeal(route(1)))
        .unwrap();
    inventory.access(identity(&fixture), |access| {
        let OutageInventoryAccess::Ready(buffer) = access else {
            panic!("not ready")
        };
        assert!(buffer.has_gap(&target).unwrap());
        assert!(!buffer.has_gap(&sibling).unwrap());
        assert_eq!(buffer.retained().count(), 0);
    });
    for kind in [
        ApprovalRequestKind::CommandExecution,
        ApprovalRequestKind::FileChange,
        ApprovalRequestKind::Permissions,
    ] {
        let request = approval_request(
            kind,
            ApprovalResponseDisposition::ResponseRequired,
            Some(route(1).thread_id().clone()),
            Some(route(1).turn_id().clone()),
            None,
        );
        let result = sink
            .submit(OrderedTurnStreamOperation::Approval(request))
            .unwrap();
        assert!(matches!(
            result,
            OrderedTurnStreamCompletion::Approval(ApprovalOperationCompletion::Unanswered { .. })
        ));
    }
    let echo = br#"{"method":"item/started","params":{"item":{"type":"userMessage","id":"passive-user","clientId":"unverified-correlation","content":[{"type":"text","text":"large payload consumed without replay","text_elements":[]}]},"threadId":"thread-1","turnId":"turn-1","startedAtMs":42}}"#;
    beryl_backend::lifecycle_test_support::decode_provider_json_for_test(echo, 3, sink.as_mut())
        .unwrap();
    assert!(control.take_checked_steering_lifecycle().is_none());
    inventory.access(identity(&fixture), |access| {
        let OutageInventoryAccess::Ready(buffer) = access else {
            panic!("not ready")
        };
        assert!(buffer.retained().any(|(_, priority, _)| priority
            == crate::cas_projection::outage_buffer::OutagePriority::UserCorrelation));
        assert!(!buffer.has_gap(&sibling).unwrap());
    });
    complete(sink.as_mut(), 2);
    drop(worker.stop_and_join().into_worker());
    let request = approval_request(
        ApprovalRequestKind::Permissions,
        ApprovalResponseDisposition::ResponseRequired,
        None,
        None,
        None,
    );
    assert!(matches!(
        sink.submit(OrderedTurnStreamOperation::Approval(request))
            .unwrap(),
        OrderedTurnStreamCompletion::Approval(ApprovalOperationCompletion::Unanswered { .. })
    ));
    drop(sink);
    drop(control);
    inventory.retire();
}

#[test]
fn rejected_inventory_and_wrong_service_cannot_be_replaced_or_bound() {
    let fixture = BrokerBuildFixture::new(206);
    let prepared = fixture.prepare_start_blocked();
    let inventory = inventory(&fixture, 0);
    let mut foreign = identity(&fixture);
    foreign.service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let foreign_inventory = Arc::new(OutageInventory::new(
        foreign,
        OutageBufferLimits {
            max_facts: 1,
            max_encoded_bytes: 1024,
            max_field_bytes: 256,
            max_targets: 1,
        },
        OutageAssemblyLimits {
            max_bytes: 1024,
            max_field_bytes: 256,
            max_entries: 8,
        },
    ));
    assert!(prepared.bind_outage_inventory(foreign_inventory).is_err());
    assert_eq!(
        inventory.publish(foreign, &[]),
        Err(OutageInventoryError::Identity)
    );
    assert_eq!(
        inventory.publish(identity(&fixture), &[target(&fixture, 1)]),
        Err(OutageInventoryError::Unrepresentable)
    );
    assert_eq!(
        inventory.publish(identity(&fixture), &[]),
        Err(OutageInventoryError::AlreadyPublished)
    );
    prepared
        .bind_outage_inventory(Arc::clone(&inventory))
        .unwrap();
    let PreparedProviderBroker {
        mut sink,
        control,
        ingester,
        start,
    } = prepared;
    let worker = ingester.start(start);
    fail_during_begin(&fixture, sink.as_mut());
    sink.submit(OrderedTurnStreamOperation::ProviderSeal(route(1)))
        .unwrap();
    complete(sink.as_mut(), 1);
    drop(worker.stop_and_join().into_worker());
    drop(sink);
    drop(control);
    inventory.access(identity(&fixture), |access| {
        assert!(matches!(access, OutageInventoryAccess::Unavailable))
    });
}

fn cancelled_seal(publication_before_join: bool) {
    let fixture = BrokerBuildFixture::new(if publication_before_join { 207 } else { 208 });
    let prepared = fixture.prepare_start_blocked();
    let inventory = inventory(&fixture, 2);
    let target = target(&fixture, 1);
    prepared
        .bind_outage_inventory(Arc::clone(&inventory))
        .unwrap();
    let PreparedProviderBroker {
        mut sink,
        control,
        ingester,
        start,
    } = prepared;
    let worker = ingester.start(start);
    fail_during_begin(&fixture, sink.as_mut());
    sink.submit(OrderedTurnStreamOperation::ProviderSeal(route(1)))
        .unwrap();
    complete(sink.as_mut(), 1);
    control.request_cancel();
    if publication_before_join {
        inventory
            .publish(identity(&fixture), &[target.clone()])
            .unwrap();
    }
    drop(worker.stop_and_join().into_worker());
    if !publication_before_join {
        inventory
            .publish(identity(&fixture), &[target.clone()])
            .unwrap();
    }
    inventory.access(identity(&fixture), |access| {
        let OutageInventoryAccess::Ready(buffer) = access else {
            panic!("inventory unavailable")
        };
        assert!(buffer.has_gap(&target).unwrap());
        assert_eq!(
            buffer.retained().count(),
            0,
            "cancelled sealed payload must not publish"
        );
    });
    drop(sink);
    drop(control);
    inventory.retire();
}

#[test]
fn cancellation_discards_sealed_payload_before_inventory_becomes_ready() {
    cancelled_seal(false);
}

#[test]
fn cancellation_prevents_ready_at_join_from_flushing_sealed_payload() {
    cancelled_seal(true);
}

#[test]
fn ready_inventory_at_finish_receives_only_loss_from_pending_seal() {
    let fixture = BrokerBuildFixture::new(209);
    let owner = inventory(&fixture, 1);
    let binding = Arc::new(std::sync::OnceLock::new());
    assert!(binding.set(Arc::clone(&owner)).is_ok());
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut passive = super::super::passive::PassiveIngress::new(
        identity(&fixture),
        fixture
            .authority
            .outage_identity(fixture.home_generation.get()),
        binding,
        Arc::new(AtomicBool::new(false)),
        Arc::clone(&cancelled),
    );
    passive.enter();
    passive.with_slot(|slot, ready| {
        assert!(ready.is_none());
        slot.begin(
            ProviderObservationId::from_bytes([209; 16]),
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::AgentMessage,
            },
            None,
        )
        .unwrap();
        let context = ProviderValueContext::Field(ProviderField::ItemId);
        slot.control(ProviderObservationControl::BeginField(context))
            .unwrap();
        slot.fragment(context, 0, "sealed-item").unwrap();
        slot.control(ProviderObservationControl::EndField(context))
            .unwrap();
        slot.seal(route(1), None).unwrap();
        assert!(slot.retained_bytes() > 0);
    });
    let target = target(&fixture, 1);
    owner
        .publish(identity(&fixture), &[target.clone()])
        .unwrap();
    cancelled.store(true, Ordering::Release);
    passive.finish();
    owner.access(identity(&fixture), |access| {
        let OutageInventoryAccess::Ready(buffer) = access else {
            panic!("not ready")
        };
        assert!(buffer.has_gap(&target).unwrap());
        assert_eq!(buffer.retained().count(), 0);
    });
}

#[test]
fn pending_loss_deduplicates_at_capacity_and_overflow_permanently_rejects_capture() {
    let fixture = BrokerBuildFixture::new(210);
    let connection = fixture
        .authority
        .outage_identity(fixture.home_generation.get());
    let owner = inventory(&fixture, 1);
    owner.record_connection_loss(identity(&fixture), connection);
    owner.record_connection_loss(identity(&fixture), connection);
    let target = target(&fixture, 1);
    owner
        .publish(identity(&fixture), &[target.clone()])
        .unwrap();
    owner.access(identity(&fixture), |access| {
        let OutageInventoryAccess::Ready(buffer) = access else {
            panic!("duplicate rejected capture")
        };
        assert!(buffer.has_gap(&target).unwrap());
    });
    let owner = inventory(&fixture, 1);
    owner.record_connection_loss(identity(&fixture), connection);
    let mut other = connection;
    other.connection += 1;
    owner.record_connection_loss(identity(&fixture), other);
    assert_eq!(
        owner.publish(identity(&fixture), &[]),
        Err(OutageInventoryError::AlreadyPublished)
    );
    owner.access(identity(&fixture), |access| {
        assert!(matches!(access, OutageInventoryAccess::Unavailable))
    });
    let owner = OutageInventory::new(
        identity(&fixture),
        OutageBufferLimits {
            max_facts: 1,
            max_encoded_bytes: 1,
            max_field_bytes: 256,
            max_targets: 10,
        },
        OutageAssemblyLimits {
            max_bytes: 1024,
            max_field_bytes: 256,
            max_entries: 8,
        },
    );
    owner.record_connection_loss(identity(&fixture), connection);
    assert_eq!(
        owner.publish(identity(&fixture), &[]),
        Err(OutageInventoryError::AlreadyPublished)
    );
    owner.access(identity(&fixture), |access| {
        assert!(matches!(access, OutageInventoryAccess::Unavailable))
    });
}
