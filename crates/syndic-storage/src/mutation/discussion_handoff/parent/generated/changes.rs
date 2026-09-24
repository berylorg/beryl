use super::*;

macro_rules! changes {
    ($($variant:ident: $family:ty),+ $(,)?) => {
        pub(super) enum Change {
            $($variant { key: <$family as Family>::Key, old: Option<<$family as Family>::Value>, new: Option<<$family as Family>::Value> }),+
        }
        impl Change {
            pub(super) fn matches(&self, reader: &impl TerminalHistoryReader) -> Result<(bool, bool), ReadError> {
                match self { $(Self::$variant { key, old, new } => {
                    let actual = reader.read::<$family>(key)?;
                    Ok((actual.as_ref() == old.as_ref(), actual.as_ref() == new.as_ref()))
                }),+ }
            }
            pub(super) fn reserve_all(changes: &[Self], reservation: &mut ReconciliationReservation<'_, SyndicDomain>) -> Result<(), SyndicMutationError> {
                $(let count = changes.iter().filter(|change| matches!(change, Self::$variant { old, new, .. } if old != new)).count();
                if count != 0 { reservation.reserve_records::<ExactCodec<$family>>(count)?; })+
                Ok(())
            }
            pub(super) fn contribute(&self, mutations: &mut MutationBuilder<'_, SyndicDomain>) -> Result<(), SyndicMutationError> {
                match self { $(Self::$variant { key, old, new } => {
                    if old != new { match new {
                        Some(value) => mutations.put::<ExactCodec<$family>>(key, value)?,
                        None => mutations.delete::<ExactCodec<$family>>(key)?,
                    } }
                }),+ }
                Ok(())
            }
        }
    };
}

changes! {
    Thread: ThreadsFamily,
    DraftIndex: DraftByThreadFamily,
    Gate: InputGatesFamily,
    NonIdle: NonIdleGateSourcesFamily,
    Input: AcceptedInputsFamily,
    Order: AcceptedOrderFamily,
    Turn: TurnsFamily,
    TurnState: TurnStatesFamily,
    Child: TurnChildrenFamily,
    Item: CanonicalItemsFamily,
    ItemIndex: TurnItemsFamily,
    Transcript: TranscriptHeadsFamily,
    TranscriptBuild: TranscriptBuildsFamily,
    Summary: HistorySummariesFamily,
    Binding: BindingsFamily,
    BindingHead: BindingHeadsFamily,
    Parent: ThreadParentFamily,
    Manifest: ContentManifestsFamily,
    Chunk: ContentChunksFamily,
    ByteSpan: ContentByteSpansFamily,
    TextSpan: ContentTextSpansFamily,
    Piece: ContentPiecesFamily,
    DraftAbsence: DraftsFamily,
    RouteAbsence: AcceptedRouteLeavesFamily,
}
