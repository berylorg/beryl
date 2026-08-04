use beryl_model::{CasThreadId, CasTurnId, InputGateRevision, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CasTurnSource, LiveSourceEvent, SourceEventPayload, SourceEventSequence, SyndicTimestamp,
    TurnStateRevision,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event = LiveSourceEvent::new(
        SyndicThreadId::from_bytes([1; 16]),
        SyndicTurnId::from_bytes([2; 16]),
        TurnStateRevision::FIRST,
        InputGateRevision::new(2)?,
        SourceEventSequence::FIRST,
        Some(CasTurnSource::new(
            CasThreadId::new("example-thread")?,
            CasTurnId::new("example-turn")?,
        )),
        SourceEventPayload::TurnActivated,
        SyndicTimestamp::from_unix_millis(3),
    )?;

    assert_eq!(event.sequence(), SourceEventSequence::FIRST);
    assert_eq!(event.payload().item_id(), None);
    Ok(())
}
