#![cfg(feature = "test-faults")]

#[path = "outage_assembly/slot.rs"]
mod slot;

use beryl_app::cas_projection::test_faults::{
    OutageAssembly, OutageAssemblyError, OutageAssemblyLimits, OutageBuffer, OutageBufferLimits,
    OutagePriority, OutageTarget, outage_test_connection_target, outage_test_target,
};
use beryl_backend::{
    ProviderEnumValue, ProviderField, ProviderItemKind, ProviderItemLifecycle,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationRoute,
    ProviderValueContext,
};
use beryl_model::{CasThreadId, CasTurnId, ProviderObservationId};

fn limits() -> OutageAssemblyLimits {
    OutageAssemblyLimits {
        max_bytes: 4096,
        max_field_bytes: 256,
        max_entries: 16,
    }
}

fn buffer(targets: &[OutageTarget]) -> OutageBuffer {
    OutageBuffer::new(
        OutageBufferLimits {
            max_facts: 100,
            max_encoded_bytes: 65536,
            max_field_bytes: 512,
            max_targets: 4,
        },
        targets,
    )
    .unwrap()
}

fn assembly(target: &OutageTarget, limits: OutageAssemblyLimits) -> OutageAssembly {
    OutageAssembly::new(
        target.connection(),
        ProviderObservationId::from_bytes([7; 16]),
        ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Completed,
            kind: ProviderItemKind::AgentMessage,
        },
        limits,
    )
}

fn route(seed: u8) -> ProviderObservationRoute {
    ProviderObservationRoute::new(
        CasThreadId::new(format!("cas-thread-{seed}")).unwrap(),
        CasTurnId::new(format!("cas-turn-{seed}")).unwrap(),
    )
}

fn field(assembly: &mut OutageAssembly, field: ProviderField, text: &str) {
    let context = ProviderValueContext::Field(field);
    assembly
        .control(ProviderObservationControl::BeginField(context))
        .unwrap();
    assembly.fragment(context, 0, text).unwrap();
    assembly
        .control(ProviderObservationControl::EndField(context))
        .unwrap();
}

#[test]
fn late_route_preserves_fragmented_utf8_and_final_priority() {
    let target = outage_test_target(1, 1);
    let mut buffer = buffer(&[target.clone()]);
    let mut assembly = assembly(&target, limits());
    field(&mut assembly, ProviderField::ItemId, "item-1");
    let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
    assembly
        .control(ProviderObservationControl::BeginField(context))
        .unwrap();
    assembly.fragment(context, 0, "Hello ").unwrap();
    assembly.fragment(context, 6, "世界").unwrap();
    assembly
        .control(ProviderObservationControl::EndField(context))
        .unwrap();
    assembly
        .control(ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(ProviderField::MessagePhase),
            value: ProviderEnumValue::FinalAnswer,
        })
        .unwrap();
    assert_eq!(buffer.retained().count(), 0);
    assembly.seal(&mut buffer, Some(&route(1))).unwrap();
    assert!(!buffer.has_gap(&target).unwrap());
    assert!(buffer.retained().any(|(_, priority, bytes)| {
        priority == OutagePriority::AssistantFinal
            && bytes
                .windows("Hello 世界".len())
                .any(|part| part == "Hello 世界".as_bytes())
    }));
}

#[test]
fn overflow_releases_payload_and_never_retains_a_suffix() {
    let target = outage_test_target(1, 1);
    let other = outage_test_connection_target(2, 2, 1);
    let mut buffer = buffer(&[target.clone(), other.clone()]);
    let mut configured = limits();
    configured.max_field_bytes = 8;
    let mut assembly = assembly(&target, configured);
    field(&mut assembly, ProviderField::ItemId, "item-1");
    let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
    assembly
        .control(ProviderObservationControl::BeginField(context))
        .unwrap();
    assembly.fragment(context, 0, "12345678").unwrap();
    assert_eq!(
        assembly.fragment(context, 8, "9"),
        Err(OutageAssemblyError::Overflow)
    );
    assert_eq!(assembly.retained_bytes(), 0);
    assert_eq!(assembly.retained_entries(), 0);
    for _ in 0..100 {
        assert_eq!(
            assembly.fragment(context, 0, "suffix"),
            Err(OutageAssemblyError::Overflow)
        );
    }
    assert_eq!(
        assembly.seal(&mut buffer, Some(&route(1))),
        Err(OutageAssemblyError::Overflow)
    );
    assert!(buffer.has_gap(&target).unwrap());
    assert!(!buffer.has_gap(&other).unwrap());
    assert_eq!(buffer.retained().count(), 0);
}

#[test]
fn unresolved_route_gaps_only_frozen_targets_of_its_connection() {
    for supplied_route in [None, Some(route(99))] {
        let first = outage_test_target(1, 1);
        let second = outage_test_connection_target(2, 2, 1);
        let unrelated = outage_test_target(3, 3);
        let mut buffer = buffer(&[first.clone(), second.clone(), unrelated.clone()]);
        let mut assembly = assembly(&first, limits());
        field(&mut assembly, ProviderField::ItemId, "item-1");
        assert!(assembly.seal(&mut buffer, supplied_route.as_ref()).is_err());
        assert!(buffer.has_gap(&first).unwrap());
        assert!(buffer.has_gap(&second).unwrap());
        assert!(!buffer.has_gap(&unrelated).unwrap());
        assert_eq!(buffer.retained().count(), 0);
    }
}

#[test]
fn field_context_and_offset_mismatch_cannot_publish() {
    for wrong_context in [false, true] {
        let target = outage_test_target(1, 1);
        let mut buffer = buffer(&[target.clone()]);
        let mut assembly = assembly(&target, limits());
        field(&mut assembly, ProviderField::ItemId, "item-1");
        let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
        assembly
            .control(ProviderObservationControl::BeginField(context))
            .unwrap();
        let supplied = if wrong_context {
            ProviderValueContext::Field(ProviderField::PlanText)
        } else {
            context
        };
        assert_eq!(
            assembly.fragment(supplied, usize::from(!wrong_context), "bad"),
            Err(OutageAssemblyError::Malformed)
        );
        assert_eq!(
            assembly.seal(&mut buffer, Some(&route(1))),
            Err(OutageAssemblyError::Malformed)
        );
        assert!(buffer.has_gap(&target).unwrap());
        assert_eq!(buffer.retained().count(), 0);
    }
}

#[test]
fn incomplete_or_duplicate_identity_never_reaches_retention() {
    for duplicate in [false, true] {
        let target = outage_test_target(1, 1);
        let mut buffer = buffer(&[target.clone()]);
        let mut assembly = assembly(&target, limits());
        field(&mut assembly, ProviderField::ItemId, "item-1");
        if duplicate {
            field(&mut assembly, ProviderField::ItemId, "item-2");
        } else {
            assembly
                .control(ProviderObservationControl::BeginField(
                    ProviderValueContext::Field(ProviderField::AgentMessageText),
                ))
                .unwrap();
        }
        assert_eq!(
            assembly.seal(&mut buffer, Some(&route(1))),
            Err(OutageAssemblyError::Malformed)
        );
        assert!(buffer.has_gap(&target).unwrap());
        assert_eq!(buffer.retained().count(), 0);
    }
}

#[test]
fn byte_and_entry_limits_apply_before_retaining_fields() {
    let target = outage_test_target(1, 1);
    for configured in [
        OutageAssemblyLimits {
            max_bytes: 0,
            ..limits()
        },
        OutageAssemblyLimits {
            max_entries: 0,
            ..limits()
        },
    ] {
        let mut assembly = assembly(&target, configured);
        assert_eq!(
            assembly.control(ProviderObservationControl::BeginField(
                ProviderValueContext::Field(ProviderField::ItemId)
            )),
            Err(OutageAssemblyError::Overflow)
        );
        assert_eq!(assembly.retained_bytes(), 0);
        assert_eq!(assembly.retained_entries(), 0);
    }
}

#[test]
fn store_failure_or_abandonment_does_not_promote_buffered_content() {
    let target = outage_test_target(1, 1);
    for abandon in [false, true] {
        let mut buffer = buffer(&[target.clone()]);
        let mut assembly = assembly(&target, limits());
        field(&mut assembly, ProviderField::ItemId, "item-1");
        field(
            &mut assembly,
            ProviderField::AgentMessageText,
            "unpublished",
        );
        if abandon {
            assembly.abandon(&mut buffer);
        } else {
            assembly.discard();
            assert_eq!(assembly.retained_bytes(), 0);
            assert_eq!(
                assembly.seal(&mut buffer, Some(&route(1))),
                Err(OutageAssemblyError::RetentionLoss)
            );
        }
        assert!(buffer.has_gap(&target).unwrap());
        assert_eq!(buffer.retained().count(), 0);
    }
}

#[test]
fn exact_byte_and_entry_boundaries_accept_then_reject_next_value() {
    let target = outage_test_target(1, 1);
    let mut probe = assembly(&target, limits());
    field(&mut probe, ProviderField::ItemId, "item-1");
    let exact = probe.retained_bytes();
    let mut bounded = assembly(
        &target,
        OutageAssemblyLimits {
            max_bytes: exact,
            max_entries: 1,
            ..limits()
        },
    );
    field(&mut bounded, ProviderField::ItemId, "item-1");
    assert_eq!(bounded.retained_bytes(), exact);
    assert_eq!(bounded.retained_entries(), 1);
    let mut target_buffer = buffer(&[target.clone()]);
    bounded.seal(&mut target_buffer, Some(&route(1))).unwrap();
    let mut below = assembly(
        &target,
        OutageAssemblyLimits {
            max_bytes: exact - 1,
            ..limits()
        },
    );
    let context = ProviderValueContext::Field(ProviderField::ItemId);
    below
        .control(ProviderObservationControl::BeginField(context))
        .unwrap();
    assert_eq!(
        below.fragment(context, 0, "item-1"),
        Err(OutageAssemblyError::Overflow)
    );
    assert_eq!(below.retained_bytes(), 0);
}

#[test]
fn structured_field_indices_cannot_change_mid_field() {
    use beryl_backend::ProviderStructuredPosition;
    let target = outage_test_target(1, 1);
    let mut assembly = assembly(&target, limits());
    let context = ProviderValueContext::Structured {
        root: ProviderField::McpResult,
        depth: 1,
        position: ProviderStructuredPosition::ListElement { index: 0 },
    };
    assembly
        .control(ProviderObservationControl::BeginField(context))
        .unwrap();
    let wrong = ProviderValueContext::Structured {
        root: ProviderField::McpResult,
        depth: 1,
        position: ProviderStructuredPosition::ListElement { index: 1 },
    };
    assert_eq!(
        assembly.fragment(wrong, 0, "bad"),
        Err(OutageAssemblyError::Malformed)
    );
    assert_eq!(assembly.retained_bytes(), 0);
}

#[test]
fn retention_saturation_reports_loss_instead_of_success() {
    let target = outage_test_target(1, 1);
    let mut buffer = OutageBuffer::new(
        OutageBufferLimits {
            max_facts: 1,
            max_encoded_bytes: 65536,
            max_field_bytes: 512,
            max_targets: 1,
        },
        &[target.clone()],
    )
    .unwrap();
    let mut assembly = assembly(&target, limits());
    field(&mut assembly, ProviderField::ItemId, "item-1");
    field(&mut assembly, ProviderField::AgentMessageText, "final");
    assert_eq!(
        assembly.seal(&mut buffer, Some(&route(1))),
        Err(OutageAssemblyError::RetentionLoss)
    );
    assert!(buffer.has_gap(&target).unwrap());
    assert_eq!(buffer.retained().count(), 1);
}

#[test]
fn command_delta_remains_operational_content() {
    use beryl_backend::ProviderDeltaKind;
    let target = outage_test_target(1, 1);
    let mut buffer = buffer(&[target.clone()]);
    let mut assembly = OutageAssembly::new(
        target.connection(),
        ProviderObservationId::from_bytes([7; 16]),
        ProviderObservationBegin::Delta {
            kind: ProviderDeltaKind::CommandExecutionOutput,
        },
        limits(),
    );
    field(&mut assembly, ProviderField::ItemId, "item-1");
    field(&mut assembly, ProviderField::DeltaText, "command-output");
    assembly.seal(&mut buffer, Some(&route(1))).unwrap();
    assert!(buffer.retained().any(|(_, priority, bytes)| {
        priority == OutagePriority::Operational
            && bytes
                .windows("command-output".len())
                .any(|part| part == b"command-output")
    }));
    assert!(
        !buffer
            .retained()
            .any(|(_, priority, _)| priority == OutagePriority::TranscriptNarrative)
    );
}

#[test]
fn reasoning_structure_retains_the_priority_of_its_narrative() {
    use beryl_backend::ProviderContainer;
    let target = outage_test_target(1, 1);
    let mut buffer = buffer(&[target.clone()]);
    let mut assembly = OutageAssembly::new(
        target.connection(),
        ProviderObservationId::from_bytes([7; 16]),
        ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Completed,
            kind: ProviderItemKind::Reasoning,
        },
        limits(),
    );
    field(&mut assembly, ProviderField::ItemId, "item-1");
    let context = ProviderValueContext::Field(ProviderField::ReasoningSummaries);
    assembly
        .control(ProviderObservationControl::BeginContainer {
            context,
            container: ProviderContainer::List,
        })
        .unwrap();
    assembly
        .control(ProviderObservationControl::BeginElement { context, index: 0 })
        .unwrap();
    field(&mut assembly, ProviderField::ReasoningSummary, "summary");
    assembly
        .control(ProviderObservationControl::EndElement { context, index: 0 })
        .unwrap();
    assembly
        .control(ProviderObservationControl::EndContainer {
            context,
            container: ProviderContainer::List,
        })
        .unwrap();
    assembly.seal(&mut buffer, Some(&route(1))).unwrap();
    assert_eq!(
        buffer
            .retained()
            .filter(|(_, priority, _)| *priority == OutagePriority::TranscriptNarrative)
            .count(),
        5
    );
    assert!(
        !buffer
            .retained()
            .any(|(_, priority, _)| priority == OutagePriority::Operational)
    );
}

#[test]
fn delta_indices_survive_as_exact_correlation_controls() {
    use beryl_backend::{ProviderDeltaKind, ProviderScalar};
    let target = outage_test_target(1, 1);
    let mut buffer = buffer(&[target.clone()]);
    let mut assembly = OutageAssembly::new(
        target.connection(),
        ProviderObservationId::from_bytes([7; 16]),
        ProviderObservationBegin::Delta {
            kind: ProviderDeltaKind::ReasoningSummaryText,
        },
        limits(),
    );
    field(&mut assembly, ProviderField::ItemId, "item-1");
    for field in [
        ProviderField::DeltaSummaryIndex,
        ProviderField::DeltaContentIndex,
    ] {
        assembly
            .control(ProviderObservationControl::Scalar {
                context: ProviderValueContext::Field(field),
                value: ProviderScalar::Unsigned(2),
            })
            .unwrap();
    }
    field(&mut assembly, ProviderField::DeltaText, "summary");
    assembly.seal(&mut buffer, Some(&route(1))).unwrap();
    assert_eq!(
        buffer
            .retained()
            .filter(|(_, priority, _)| *priority == OutagePriority::IdentityCorrelation)
            .count(),
        5
    );
    assert!(
        !buffer
            .retained()
            .any(|(_, priority, _)| priority == OutagePriority::Operational)
    );
}

#[test]
fn trailing_route_selects_the_other_frozen_target_on_the_connection() {
    let first = outage_test_target(1, 1);
    let second = outage_test_connection_target(2, 2, 1);
    let mut buffer = buffer(&[first.clone(), second.clone()]);
    let mut assembly = assembly(&first, limits());
    field(&mut assembly, ProviderField::ItemId, "item-2");
    field(
        &mut assembly,
        ProviderField::AgentMessageText,
        "second target",
    );
    assembly.seal(&mut buffer, Some(&route(2))).unwrap();
    assert!(buffer.retained().all(|(target, _, _)| target == &second));
    assert!(!buffer.has_gap(&first).unwrap());
    assert!(!buffer.has_gap(&second).unwrap());
}

#[test]
fn ambiguous_route_never_selects_one_of_its_targets() {
    use beryl_app::cas_projection::test_faults::outage_test_routed_target;
    let first = outage_test_target(1, 1);
    let second = outage_test_routed_target(2, 2, 1, 1, 1);
    let mut buffer = buffer(&[first.clone(), second.clone()]);
    let mut assembly = assembly(&first, limits());
    field(&mut assembly, ProviderField::ItemId, "item-1");
    assert_eq!(
        assembly.seal(&mut buffer, Some(&route(1))),
        Err(OutageAssemblyError::UnqualifiedRoute)
    );
    assert!(buffer.has_gap(&first).unwrap());
    assert!(buffer.has_gap(&second).unwrap());
    assert_eq!(buffer.retained().count(), 0);
}

#[test]
fn old_assembly_cannot_capture_or_gap_replacement_generation() {
    use beryl_app::cas_projection::test_faults::outage_test_routed_target;
    let old = outage_test_target(1, 1);
    let replacement = outage_test_routed_target(1, 2, 1, 1, 2);
    let mut buffer = buffer(&[replacement.clone()]);
    let mut assembly = assembly(&old, limits());
    field(&mut assembly, ProviderField::ItemId, "item-1");
    assert_eq!(
        assembly.seal(&mut buffer, Some(&route(1))),
        Err(OutageAssemblyError::UnqualifiedRoute)
    );
    assert!(!buffer.has_gap(&replacement).unwrap());
    assert_eq!(buffer.retained().count(), 0);
}
