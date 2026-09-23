use super::*;
use beryl_app::cas_projection::test_faults::OutageObservationSlot;

fn begin(slot: &mut OutageObservationSlot, ready: Option<&mut OutageBuffer>) {
    slot.begin(
        ProviderObservationId::from_bytes([8; 16]),
        ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Completed,
            kind: ProviderItemKind::AgentMessage,
        },
        ready,
    )
    .unwrap();
}

fn field(slot: &mut OutageObservationSlot, field: ProviderField, text: &str) {
    let context = ProviderValueContext::Field(field);
    slot.control(ProviderObservationControl::BeginField(context))
        .unwrap();
    slot.fragment(context, 0, text).unwrap();
    slot.control(ProviderObservationControl::EndField(context))
        .unwrap();
}

fn complete(slot: &mut OutageObservationSlot, text: &str) {
    field(slot, ProviderField::ItemId, "item");
    field(slot, ProviderField::AgentMessageText, text);
    slot.seal(route(1), None).unwrap();
}

#[test]
fn pending_seals_reuse_one_slot_and_attribute_eviction_only_to_the_connection() {
    let target = outage_test_target(1, 1);
    let sibling = outage_test_connection_target(2, 2, 1);
    let foreign = outage_test_target(3, 3);
    let mut slot = OutageObservationSlot::new(target.connection(), limits());
    for _ in 0..20 {
        begin(&mut slot, None);
        complete(&mut slot, "newest");
        assert!(slot.retained_bytes() <= limits().max_bytes);
        assert_eq!(slot.retained_entries(), 3);
    }
    let mut buffer = buffer(&[target.clone(), sibling.clone(), foreign.clone()]);
    slot.flush(&mut buffer);
    assert_eq!(slot.retained_bytes(), 0);
    assert_eq!(slot.retained_entries(), 0);
    assert_eq!(buffer.retained().count(), 4);
    assert!(buffer.has_gap(&target).unwrap());
    assert!(buffer.has_gap(&sibling).unwrap());
    assert!(!buffer.has_gap(&foreign).unwrap());
    slot.flush(&mut buffer);
    assert_eq!(buffer.retained().count(), 4);
}

#[test]
fn ready_inventory_hands_off_before_the_next_begin_without_loss() {
    let target = outage_test_target(1, 1);
    let mut slot = OutageObservationSlot::new(target.connection(), limits());
    begin(&mut slot, None);
    complete(&mut slot, "first");
    let mut buffer = buffer(&[target.clone()]);
    begin(&mut slot, Some(&mut buffer));
    assert_eq!(buffer.retained().count(), 4);
    assert_eq!(slot.retained_entries(), 0);
    complete(&mut slot, "second");
    slot.flush(&mut buffer);
    assert_eq!(buffer.retained().count(), 8);
    assert!(!buffer.has_gap(&target).unwrap());
}

#[test]
fn pending_loss_survives_inventory_and_disable_releases_all_scratch() {
    let target = outage_test_target(1, 1);
    let mut slot = OutageObservationSlot::new(target.connection(), limits());
    slot.record_loss();
    begin(&mut slot, None);
    complete(&mut slot, "discarded");
    slot.disable();
    assert_eq!(slot.retained_bytes(), 0);
    assert_eq!(slot.retained_entries(), 0);
    let mut buffer = buffer(&[target.clone()]);
    slot.flush(&mut buffer);
    assert!(buffer.has_gap(&target).unwrap());
    assert_eq!(buffer.retained().count(), 0);
    assert_eq!(
        slot.begin(
            ProviderObservationId::from_bytes([9; 16]),
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::AgentMessage,
            },
            Some(&mut buffer),
        ),
        Err(OutageAssemblyError::RetentionLoss)
    );
}

#[test]
fn sealed_route_is_charged_to_the_same_byte_and_entry_limits() {
    let target = outage_test_target(1, 1);
    for entry_limit in [2, 16] {
        let mut configured = limits();
        configured.max_entries = entry_limit;
        configured.max_field_bytes = if entry_limit == 2 { 256 } else { 8 };
        let mut slot = OutageObservationSlot::new(target.connection(), configured);
        begin(&mut slot, None);
        field(&mut slot, ProviderField::ItemId, "item");
        field(&mut slot, ProviderField::AgentMessageText, "text");
        let long_route = ProviderObservationRoute::new(
            CasThreadId::new("r".repeat(9)).unwrap(),
            CasTurnId::new("turn").unwrap(),
        );
        let route = if entry_limit == 2 {
            route(1)
        } else {
            long_route
        };
        assert_eq!(slot.seal(route, None), Err(OutageAssemblyError::Overflow));
        assert_eq!(slot.retained_bytes(), 0);
        let mut buffer = buffer(&[target.clone()]);
        slot.flush(&mut buffer);
        assert!(buffer.has_gap(&target).unwrap());
        assert_eq!(buffer.retained().count(), 0);
    }
}

#[test]
fn malformed_unsealed_fields_never_become_pending_complete_facts() {
    let target = outage_test_target(1, 1);
    let mut slot = OutageObservationSlot::new(target.connection(), limits());
    begin(&mut slot, None);
    slot.control(ProviderObservationControl::BeginField(
        ProviderValueContext::Field(ProviderField::ItemId),
    ))
    .unwrap();
    slot.fragment(
        ProviderValueContext::Field(ProviderField::ItemId),
        0,
        "partial",
    )
    .unwrap();
    assert_eq!(
        slot.seal(route(1), None),
        Err(OutageAssemblyError::Malformed)
    );
    assert_eq!(slot.retained_bytes(), 0);
    let mut buffer = buffer(&[target.clone()]);
    slot.flush(&mut buffer);
    assert_eq!(buffer.retained().count(), 0);
    assert!(buffer.has_gap(&target).unwrap());
}

#[test]
fn seal_accepts_exact_aggregate_bytes_and_releases_on_one_byte_overflow() {
    let target = outage_test_target(1, 1);
    let mut measured = OutageObservationSlot::new(target.connection(), limits());
    begin(&mut measured, None);
    complete(&mut measured, "text");
    let required = measured.retained_bytes();
    for max_bytes in [required, required - 1] {
        let mut configured = limits();
        configured.max_bytes = max_bytes;
        let mut slot = OutageObservationSlot::new(target.connection(), configured);
        begin(&mut slot, None);
        field(&mut slot, ProviderField::ItemId, "item");
        field(&mut slot, ProviderField::AgentMessageText, "text");
        let result = slot.seal(route(1), None);
        if max_bytes == required {
            assert_eq!(result, Ok(()));
            assert_eq!(slot.retained_bytes(), required);
        } else {
            assert_eq!(result, Err(OutageAssemblyError::Overflow));
            assert_eq!(slot.retained_bytes(), 0);
        }
    }
}

#[test]
fn qualified_retention_loss_does_not_gap_other_connection_targets() {
    let target = outage_test_target(1, 1);
    let sibling = outage_test_connection_target(2, 2, 1);
    let mut slot = OutageObservationSlot::new(target.connection(), limits());
    begin(&mut slot, None);
    complete(&mut slot, "text");
    let mut buffer = OutageBuffer::new(
        OutageBufferLimits {
            max_facts: 0,
            max_encoded_bytes: 65536,
            max_field_bytes: 512,
            max_targets: 4,
        },
        &[target.clone(), sibling.clone()],
    )
    .unwrap();
    slot.flush(&mut buffer);
    assert!(buffer.has_gap(&target).unwrap());
    assert!(!buffer.has_gap(&sibling).unwrap());
    assert_eq!(slot.retained_bytes(), 0);
}

#[test]
fn malformed_operation_after_seal_abandons_pending_ownership() {
    let target = outage_test_target(1, 1);
    let mut slot = OutageObservationSlot::new(target.connection(), limits());
    begin(&mut slot, None);
    complete(&mut slot, "text");
    assert_eq!(
        slot.control(ProviderObservationControl::BeginField(
            ProviderValueContext::Field(ProviderField::ItemId),
        )),
        Err(OutageAssemblyError::Malformed)
    );
    assert_eq!(slot.retained_bytes(), 0);
    let mut buffer = buffer(&[target.clone()]);
    slot.flush(&mut buffer);
    assert_eq!(buffer.retained().count(), 0);
    assert!(buffer.has_gap(&target).unwrap());
}

#[test]
fn overflow_with_ready_trailing_route_gaps_only_its_qualified_target() {
    let target = outage_test_target(1, 1);
    let sibling = outage_test_connection_target(2, 2, 1);
    let mut configured = limits();
    configured.max_field_bytes = 16;
    let mut slot = OutageObservationSlot::new(target.connection(), configured);
    let mut buffer = buffer(&[target.clone(), sibling.clone()]);
    begin(&mut slot, Some(&mut buffer));
    field(&mut slot, ProviderField::ItemId, "item");
    let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
    slot.control(ProviderObservationControl::BeginField(context))
        .unwrap();
    assert_eq!(
        slot.fragment(context, 0, "overflow payload!"),
        Err(OutageAssemblyError::Overflow)
    );
    assert_eq!(
        slot.control(ProviderObservationControl::EndField(context)),
        Err(OutageAssemblyError::Overflow)
    );
    assert_eq!(
        slot.seal(route(1), Some(&mut buffer)),
        Err(OutageAssemblyError::Overflow)
    );
    slot.flush(&mut buffer);
    assert!(buffer.has_gap(&target).unwrap());
    assert!(!buffer.has_gap(&sibling).unwrap());
    assert_eq!(buffer.retained().count(), 0);
    assert_eq!(slot.retained_bytes(), 0);
}
