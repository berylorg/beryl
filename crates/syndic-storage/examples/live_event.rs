use beryl_model::{
    CasItemId, CasThreadId, CasTurnId, InputGateRevision, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    AssistantMessagePhase, CasTurnSource, LiveSourceEvent, ProviderItemDisposition,
    ProviderItemKind, SourceEventPayload, SourceEventSequence, SourceItemDescriptor,
    SyndicTimestamp, TurnStateRevision,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let item = SyndicItemId::from_bytes([3; 16]);
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
        SourceEventPayload::ItemStarted {
            item: SourceItemDescriptor::new(
                item,
                CasItemId::new("example-item")?,
                ProviderItemKind::AgentMessage,
                ProviderItemDisposition::CanonicalText,
            )?,
            assistant_phase: Some(AssistantMessagePhase::Unknown),
        },
        SyndicTimestamp::from_unix_millis(3),
    )?;

    assert_eq!(event.sequence(), SourceEventSequence::FIRST);
    assert_eq!(event.payload().item_id(), Some(item));
    Ok(())
}
