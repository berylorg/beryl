#![cfg(feature = "test-faults")]

use beryl_app::cas_projection::test_faults::{
    OutageBuffer, OutageBufferError, OutageBufferLimits, OutageFact, OutageLoss, OutagePriority,
    OutageTextKind, outage_test_target,
};
use beryl_backend::{
    ClientUserMessageId, NormalTurnTerminalStatus, ProviderField, ProviderObservationControl,
    ProviderValueContext,
};
use beryl_model::{CasItemId, ProviderObservationId};

fn limits(facts: usize, bytes: usize, field: usize, targets: usize) -> OutageBufferLimits {
    OutageBufferLimits {
        max_facts: facts,
        max_encoded_bytes: bytes,
        max_field_bytes: field,
        max_targets: targets,
    }
}

fn observation(seed: u8) -> ProviderObservationId {
    ProviderObservationId::from_bytes([seed; 16])
}

fn item(seed: u8) -> CasItemId {
    CasItemId::new(format!("item-{seed}")).unwrap()
}

fn text<'a>(seed: u8, kind: OutageTextKind, value: &'a str) -> OutageFact<'a> {
    OutageFact::CompleteField {
        observation: observation(seed),
        ordinal: u64::from(seed),
        context: ProviderValueContext::Field(ProviderField::AgentMessageText),
        kind,
        text: value,
    }
}

fn priorities(buffer: &OutageBuffer) -> Vec<OutagePriority> {
    buffer.retained().map(|(_, priority, _)| priority).collect()
}

#[test]
fn frozen_targets_enforce_count_alias_and_metadata_field_bounds() {
    let first = outage_test_target(1, 10);
    let same_syndic_identity = outage_test_target(1, 11);
    let second = outage_test_target(2, 12);

    assert!(matches!(
        OutageBuffer::new(limits(4, 512, 16, 1), &[first.clone(), second]),
        Err(OutageBufferError::TargetLimit)
    ));
    assert!(matches!(
        OutageBuffer::new(limits(4, 512, 16, 2), &[first.clone(), first.clone()]),
        Err(OutageBufferError::DuplicateTarget)
    ));
    assert!(matches!(
        OutageBuffer::new(limits(4, 512, 16, 2), &[first, same_syndic_identity]),
        Err(OutageBufferError::DuplicateTarget)
    ));
    assert!(matches!(
        OutageBuffer::new(limits(4, 512, 8, 1), &[outage_test_target(3, 13)]),
        Err(OutageBufferError::FieldLimit)
    ));
}

#[test]
fn target_metadata_uses_its_exact_encoded_byte_boundary() {
    let target = outage_test_target(1, 14);
    let exact_target_bytes = 16 * 3 + 8 * 6 + 8 * 2 + "cas-thread-1".len() + "cas-turn-1".len() + 1;
    let buffer =
        OutageBuffer::new(limits(1, exact_target_bytes, 16, 1), &[target.clone()]).unwrap();
    assert_eq!(buffer.encoded_bytes(), exact_target_bytes);
    assert!(matches!(
        OutageBuffer::new(limits(1, exact_target_bytes - 1, 16, 1), &[target]),
        Err(OutageBufferError::EncodedByteLimit)
    ));
}

#[test]
fn item_and_encoded_byte_bounds_are_independent_and_atomic() {
    let target = outage_test_target(4, 20);
    let mut probe = OutageBuffer::new(limits(4, 4096, 64, 1), &[target.clone()]).unwrap();
    let overhead = probe.encoded_bytes();
    probe
        .offer(&target, text(4, OutageTextKind::Operational, "one"))
        .unwrap();
    let fact_bytes = probe.encoded_bytes() - overhead;

    let mut item_bound = OutageBuffer::new(limits(1, 4096, 64, 1), &[target.clone()]).unwrap();
    item_bound
        .offer(&target, text(5, OutageTextKind::Operational, "one"))
        .unwrap();
    assert_eq!(
        item_bound.offer(&target, text(6, OutageTextKind::Operational, "two")),
        Err(OutageBufferError::FactLimit)
    );
    assert_eq!(priorities(&item_bound), vec![OutagePriority::Operational]);

    let mut byte_bound =
        OutageBuffer::new(limits(4, overhead + fact_bytes, 64, 1), &[target.clone()]).unwrap();
    byte_bound
        .offer(&target, text(4, OutageTextKind::Operational, "one"))
        .unwrap();
    assert_eq!(
        byte_bound.offer(&target, text(5, OutageTextKind::Operational, "two")),
        Err(OutageBufferError::EncodedByteLimit)
    );
    assert_eq!(byte_bound.encoded_bytes(), overhead + fact_bytes);
}

#[test]
fn field_limits_cover_every_borrowed_text_and_preserve_exact_utf8() {
    let target = outage_test_target(5, 30);
    let oversized = CasItemId::new("x".repeat(17)).unwrap();
    let small = item(55);
    let client =
        ClientUserMessageId::try_new("beryl.accepted-input.v1:00010a0f10117f809aafb0cddeeffeff")
            .unwrap();
    let oversized_text = "x".repeat(17);
    let mut buffer = OutageBuffer::new(limits(8, 4096, 16, 1), &[target.clone()]).unwrap();
    let before = buffer.encoded_bytes();
    for fact in [
        OutageFact::Identity {
            observation: observation(6),
            item: &oversized,
        },
        text(7, OutageTextKind::TranscriptNarrative, &oversized_text),
        OutageFact::UserCorrelation {
            item: &small,
            client: &client,
        },
        OutageFact::MediaHandoff {
            item: &small,
            saved_path: &oversized_text,
        },
    ] {
        assert_eq!(
            buffer.offer(&target, fact),
            Err(OutageBufferError::FieldLimit)
        );
        assert_eq!(buffer.encoded_bytes(), before);
        assert_eq!(buffer.retained().count(), 0);
    }

    let utf8 = "\u{00e9}".repeat(8);
    let mut utf8_buffer = OutageBuffer::new(limits(1, 4096, 32, 1), &[target.clone()]).unwrap();
    utf8_buffer
        .offer(&target, text(8, OutageTextKind::TranscriptNarrative, &utf8))
        .unwrap();
    assert!(
        utf8_buffer
            .retained()
            .next()
            .unwrap()
            .2
            .ends_with(utf8.as_bytes())
    );
    let over_utf8 = "\u{00e9}".repeat(9);
    let mut utf8_limit = OutageBuffer::new(limits(1, 4096, 16, 1), &[target.clone()]).unwrap();
    utf8_limit
        .offer(&target, text(9, OutageTextKind::TranscriptNarrative, &utf8))
        .unwrap();
    assert_eq!(
        utf8_limit.offer(
            &target,
            text(10, OutageTextKind::TranscriptNarrative, &over_utf8)
        ),
        Err(OutageBufferError::FieldLimit)
    );
}

#[test]
fn every_priority_is_retained_and_higher_priority_evicts_the_exact_lowest_payload() {
    let target = outage_test_target(6, 40);
    let identity = item(1);
    let user = item(2);
    let media = item(3);
    let client =
        ClientUserMessageId::try_new("beryl.accepted-input.v1:00010a0f10117f809aafb0cddeeffeff")
            .unwrap();
    let mut all = OutageBuffer::new(limits(7, 4096, 128, 1), &[target.clone()]).unwrap();
    all.offer(
        &target,
        OutageFact::Identity {
            observation: observation(1),
            item: &identity,
        },
    )
    .unwrap();
    all.offer(
        &target,
        OutageFact::Terminal(NormalTurnTerminalStatus::Completed),
    )
    .unwrap();
    all.offer(
        &target,
        text(2, OutageTextKind::AssistantFinal, "assistant"),
    )
    .unwrap();
    all.offer(
        &target,
        text(3, OutageTextKind::TranscriptNarrative, "narrative"),
    )
    .unwrap();
    all.offer(
        &target,
        OutageFact::UserCorrelation {
            item: &user,
            client: &client,
        },
    )
    .unwrap();
    all.offer(
        &target,
        OutageFact::MediaHandoff {
            item: &media,
            saved_path: "media",
        },
    )
    .unwrap();
    all.offer(&target, text(4, OutageTextKind::Operational, "operational"))
        .unwrap();
    assert_eq!(
        priorities(&all),
        vec![
            OutagePriority::IdentityCorrelation,
            OutagePriority::Terminal,
            OutagePriority::AssistantFinal,
            OutagePriority::TranscriptNarrative,
            OutagePriority::UserCorrelation,
            OutagePriority::MediaHandoff,
            OutagePriority::Operational,
        ]
    );

    let mut eviction = OutageBuffer::new(limits(6, 4096, 128, 1), &[target.clone()]).unwrap();
    eviction
        .offer(
            &target,
            OutageFact::Terminal(NormalTurnTerminalStatus::Completed),
        )
        .unwrap();
    eviction
        .offer(
            &target,
            text(2, OutageTextKind::AssistantFinal, "assistant"),
        )
        .unwrap();
    eviction
        .offer(
            &target,
            text(3, OutageTextKind::TranscriptNarrative, "narrative"),
        )
        .unwrap();
    eviction
        .offer(
            &target,
            OutageFact::UserCorrelation {
                item: &user,
                client: &client,
            },
        )
        .unwrap();
    eviction
        .offer(
            &target,
            OutageFact::MediaHandoff {
                item: &media,
                saved_path: "media",
            },
        )
        .unwrap();
    eviction
        .offer(&target, text(4, OutageTextKind::Operational, "operational"))
        .unwrap();
    eviction
        .offer(
            &target,
            OutageFact::Identity {
                observation: observation(9),
                item: &identity,
            },
        )
        .unwrap();
    let retained: Vec<_> = eviction.retained().collect();
    assert!(
        !retained
            .iter()
            .any(|(_, _, value)| value.ends_with(b"operational"))
    );
    assert!(retained.iter().any(|(_, priority, value)| *priority
        == OutagePriority::IdentityCorrelation
        && value.ends_with(b"item-1")));
    assert!(eviction.has_gap(&target).unwrap());
}

#[test]
fn same_priority_and_impossible_higher_priority_do_not_evict_before_admission() {
    let target = outage_test_target(7, 50);
    let mut same = OutageBuffer::new(limits(1, 4096, 128, 1), &[target.clone()]).unwrap();
    same.offer(&target, text(1, OutageTextKind::Operational, "first"))
        .unwrap();
    let before_same = same.encoded_bytes();
    assert_eq!(
        same.offer(&target, text(2, OutageTextKind::Operational, "second")),
        Err(OutageBufferError::FactLimit)
    );
    assert_eq!(same.encoded_bytes(), before_same);
    assert!(same.retained().next().unwrap().2.ends_with(b"first"));

    let huge = "x".repeat(40);
    let mut probe = OutageBuffer::new(limits(2, 4096, 128, 1), &[target.clone()]).unwrap();
    let overhead = probe.encoded_bytes();
    probe
        .offer(&target, text(3, OutageTextKind::AssistantFinal, &huge))
        .unwrap();
    let incoming = probe.encoded_bytes() - overhead;
    let mut impossible = OutageBuffer::new(
        limits(2, overhead + incoming - 1, 128, 1),
        &[target.clone()],
    )
    .unwrap();
    impossible
        .offer(&target, text(4, OutageTextKind::Operational, "kept"))
        .unwrap();
    let before = impossible.encoded_bytes();
    assert_eq!(
        impossible.offer(&target, text(5, OutageTextKind::AssistantFinal, &huge)),
        Err(OutageBufferError::EncodedByteLimit)
    );
    assert_eq!(impossible.encoded_bytes(), before);
    assert!(impossible.retained().next().unwrap().2.ends_with(b"kept"));
}

#[test]
fn cross_target_eviction_gaps_only_the_evicted_target() {
    let victim = outage_test_target(13, 90);
    let incoming = outage_test_target(14, 91);
    let unrelated = outage_test_target(15, 92);
    let mut buffer = OutageBuffer::new(
        limits(2, 4096, 128, 3),
        &[victim.clone(), incoming.clone(), unrelated.clone()],
    )
    .unwrap();
    buffer
        .offer(&victim, text(1, OutageTextKind::Operational, "victim"))
        .unwrap();
    buffer
        .offer(
            &unrelated,
            text(2, OutageTextKind::TranscriptNarrative, "unrelated"),
        )
        .unwrap();
    buffer
        .offer(
            &incoming,
            OutageFact::Terminal(NormalTurnTerminalStatus::Completed),
        )
        .unwrap();
    let retained: Vec<_> = buffer.retained().collect();
    assert!(
        !retained
            .iter()
            .any(|(_, _, value)| value.ends_with(b"victim"))
    );
    assert!(retained.iter().any(|(_, priority, value)| *priority
        == OutagePriority::TranscriptNarrative
        && value.ends_with(b"unrelated")));
    assert!(
        retained
            .iter()
            .any(|(_, priority, _)| *priority == OutagePriority::Terminal)
    );
    assert!(buffer.has_gap(&victim).unwrap());
    assert!(!buffer.has_gap(&incoming).unwrap());
    assert!(!buffer.has_gap(&unrelated).unwrap());
}

#[test]
fn control_encoding_retains_priority_and_distinguishes_ordinal_and_context() {
    let target = outage_test_target(8, 60);
    let mut buffer = OutageBuffer::new(limits(2, 4096, 128, 1), &[target.clone()]).unwrap();
    buffer
        .offer(
            &target,
            OutageFact::Control {
                observation: observation(1),
                ordinal: 1,
                priority: OutagePriority::Operational,
                control: ProviderObservationControl::BeginField(ProviderValueContext::Field(
                    ProviderField::ItemId,
                )),
            },
        )
        .unwrap();
    buffer
        .offer(
            &target,
            OutageFact::Control {
                observation: observation(1),
                ordinal: 2,
                priority: OutagePriority::IdentityCorrelation,
                control: ProviderObservationControl::BeginField(ProviderValueContext::Field(
                    ProviderField::AgentMessageText,
                )),
            },
        )
        .unwrap();
    let facts: Vec<_> = buffer.retained().collect();
    assert_eq!(
        facts
            .iter()
            .map(|(_, priority, _)| *priority)
            .collect::<Vec<_>>(),
        vec![
            OutagePriority::Operational,
            OutagePriority::IdentityCorrelation
        ]
    );
    assert_ne!(facts[0].2, facts[1].2);
}

#[test]
fn terminal_status_is_part_of_the_retained_encoding() {
    let target = outage_test_target(12, 61);
    let mut buffer = OutageBuffer::new(limits(2, 4096, 128, 1), &[target.clone()]).unwrap();
    buffer
        .offer(
            &target,
            OutageFact::Terminal(NormalTurnTerminalStatus::Completed),
        )
        .unwrap();
    buffer
        .offer(
            &target,
            OutageFact::Terminal(NormalTurnTerminalStatus::Interrupted),
        )
        .unwrap();
    let retained: Vec<_> = buffer.retained().collect();
    assert_eq!(
        priorities(&buffer),
        vec![OutagePriority::Terminal, OutagePriority::Terminal]
    );
    assert_ne!(retained[0].2, retained[1].2);
}

#[test]
fn loss_is_sticky_for_its_target_and_changed_registration_is_foreign() {
    let first = outage_test_target(9, 70);
    let second = outage_test_target(10, 71);
    let changed_registration = outage_test_target(9, 72);
    let mut buffer =
        OutageBuffer::new(limits(4, 4096, 128, 2), &[first.clone(), second.clone()]).unwrap();
    assert_eq!(
        buffer.offer(
            &changed_registration,
            OutageFact::Terminal(NormalTurnTerminalStatus::Completed)
        ),
        Err(OutageBufferError::UnknownTarget)
    );
    assert!(!buffer.has_gap(&first).unwrap());
    assert!(!buffer.has_gap(&second).unwrap());
    for loss in [
        OutageLoss::Partial,
        OutageLoss::Unrepresentable,
        OutageLoss::Dropped,
    ] {
        buffer.record_loss(&first, loss).unwrap();
    }
    buffer
        .offer(
            &first,
            OutageFact::Terminal(NormalTurnTerminalStatus::Completed),
        )
        .unwrap();
    assert!(buffer.has_gap(&first).unwrap());
    assert!(!buffer.has_gap(&second).unwrap());
}

#[test]
fn retirement_consumes_the_transient_buffer() {
    let target = outage_test_target(11, 80);
    let mut buffer = OutageBuffer::new(limits(1, 4096, 128, 1), &[target.clone()]).unwrap();
    buffer
        .offer(
            &target,
            OutageFact::Terminal(NormalTurnTerminalStatus::Completed),
        )
        .unwrap();
    assert_eq!(buffer.retained().count(), 1);
    buffer.retire();
}
