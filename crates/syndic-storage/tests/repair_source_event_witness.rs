use beryl_model::{CasThreadId, CasTurnId, SyndicTurnId};
use syndic_storage::{
    CasTurnSource, SourceEventPayload, SourceEventRecord, SourceEventSequence, TurnEndStatus,
    TurnIncompleteReason, TurnTerminalOutcome, UnsupportedHistoryReason,
};

fn source(thread: &str, turn: &str) -> CasTurnSource {
    CasTurnSource::new(
        CasThreadId::new(thread).unwrap(),
        CasTurnId::new(turn).unwrap(),
    )
}

fn event(
    turn: [u8; 16],
    sequence: u64,
    source: Option<CasTurnSource>,
    payload: SourceEventPayload,
) -> SourceEventRecord {
    SourceEventRecord::new(
        SyndicTurnId::from_bytes(turn),
        SourceEventSequence::new(sequence).unwrap(),
        source,
        payload,
    )
    .unwrap()
}

fn digest_hex(event: &SourceEventRecord) -> String {
    event
        .repair_witness()
        .digest()
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn canonical_terminal_vectors_cover_domain_byte_order_and_utf8() {
    let turn = std::array::from_fn(|index| index as u8);
    let uncorrelated = event(
        turn,
        0x0102030405060708,
        None,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
    );
    assert_eq!(
        digest_hex(&uncorrelated),
        "7cfc137f061c736aba028e17ceffdb57e30f69922724d5d72850a4284ab2f02c"
    );
    let correlated = event(
        turn,
        u64::MAX,
        Some(source("Th", "Tü")),
        SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
            TurnIncompleteReason::UnsupportedHistory(
                UnsupportedHistoryReason::HostedImageGeneration,
            ),
        )),
    );
    assert_eq!(
        digest_hex(&correlated),
        "1e450c1de72afd55af4d8a2ab924d8ce1ed724c63ca8f8ad0a3ec3bcb2607694"
    );
    for record in [uncorrelated, correlated] {
        assert_eq!(record.repair_witness().sequence(), record.sequence());
        assert_eq!(record.clone().repair_witness(), record.repair_witness());
    }
}

#[test]
fn witness_commits_each_target_field_and_terminal_payload() {
    let base = event(
        [7; 16],
        1,
        Some(source("thread", "turn")),
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
    );
    let variants = [
        event([8; 16], 1, base.source().cloned(), base.payload().clone()),
        event([7; 16], 2, base.source().cloned(), base.payload().clone()),
        event(
            [7; 16],
            1,
            Some(source("Thread", "turn")),
            base.payload().clone(),
        ),
        event(
            [7; 16],
            1,
            Some(source("thread", "Turn")),
            base.payload().clone(),
        ),
        event([7; 16], 1, None, base.payload().clone()),
        event(
            [7; 16],
            1,
            base.source().cloned(),
            SourceEventPayload::TurnActivated,
        ),
        event(
            [7; 16],
            1,
            base.source().cloned(),
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
            ),
        ),
        event(
            [7; 16],
            1,
            base.source().cloned(),
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(
                    TurnTerminalOutcome::Complete,
                    Some(TurnIncompleteReason::StreamLost),
                )
                .unwrap(),
            ),
        ),
    ];
    for variant in variants {
        assert_ne!(
            base.repair_witness().digest(),
            variant.repair_witness().digest()
        );
    }
}

#[test]
fn descriptive_hashing_does_not_require_terminal_eligibility_or_storage() {
    let unknown = event(
        [0; 16],
        1,
        None,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::UnknownTerminal, None).unwrap(),
        ),
    );
    let active = event(
        [0; 16],
        1,
        Some(source(&"é".repeat(128), &"t".repeat(256))),
        SourceEventPayload::TurnActivated,
    );
    assert_ne!(
        unknown.repair_witness().digest(),
        active.repair_witness().digest()
    );
    assert_eq!(
        unknown.repair_witness().sequence(),
        SourceEventSequence::FIRST
    );
}
