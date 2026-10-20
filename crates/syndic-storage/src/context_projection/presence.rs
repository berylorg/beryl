use crate::{
    CasLineageProof, CasRepresentedPrefixProof, SyndicReadError, SyndicStorage,
    TurnDispatchProvenance, codec::*, domain::SyndicDomain,
};
use beryl_home_store::{DomainReader, HomeStore, PointReadLimit};
use beryl_model::SyndicThreadId;

pub(crate) enum ContextPresenceAccess<'a, 'b> {
    Live(&'a SyndicStorage, &'a HomeStore),
    Writer(&'a DomainReader<'b, SyndicDomain>),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextPresence {
    Absent,
    Required,
    Unclassified,
}
impl ContextPresenceAccess<'_, '_> {
    fn required_record<F: Family>(&self, key: F::Key) -> Result<F::Value, SyndicReadError> {
        let value = match self {
            Self::Live(storage, store) => storage.point::<F>(store, key, super::read::point())?,
            Self::Writer(reader) => reader.point::<ExactCodec<F>>(
                &key,
                PointReadLimit::new(66_048).expect("context presence point bound"),
            )?,
        };
        value.ok_or(SyndicReadError::Invariant(
            "context presence correlation record missing",
        ))
    }
    pub(crate) fn classify(
        &self,
        target: SyndicThreadId,
        prefix: CasRepresentedPrefixProof,
    ) -> Result<ContextPresence, SyndicReadError> {
        let target = self.required_record::<ThreadsFamily>(target)?;
        let Some(tail_id) = prefix.tail() else {
            return Ok(if target.context_owner_id().is_some() {
                ContextPresence::Required
            } else {
                ContextPresence::Absent
            });
        };
        let tail = self.required_record::<TurnsFamily>(tail_id)?;
        if tail.chain_digest() != prefix.digest() {
            return Err(SyndicReadError::Invariant(
                "context presence prefix digest disagrees",
            ));
        }
        let origin = self.required_record::<ThreadsFamily>(tail.origin_thread_id())?;
        let state = self.required_record::<TurnStatesFamily>(tail_id)?;
        let snapshot = match state.dispatch_provenance() {
            TurnDispatchProvenance::Activated(anchor)
            | TurnDispatchProvenance::Cancelled(anchor) => {
                let snapshot =
                    self.required_record::<ExecutionSnapshotsFamily>(anchor.snapshot_id())?;
                let binding = self.required_record::<BindingsFamily>(BindingKey {
                    thread: tail.origin_thread_id(),
                    revision: anchor.binding_revision(),
                })?;
                if !crate::dispatch_provenance::activation_matches(
                    tail.origin_thread_id(),
                    tail_id,
                    anchor,
                    &snapshot,
                    &binding,
                ) {
                    return Err(SyndicReadError::Invariant(
                        "context presence original activation does not correlate",
                    ));
                }
                Some(snapshot)
            }
            TurnDispatchProvenance::ProviderOperation => {
                let ordinal =
                    crate::SourceEventSequence::new(state.source_event_count()).map_err(|_| {
                        SyndicReadError::Invariant(
                            "provider context presence has no terminal source",
                        )
                    })?;
                let event = self.required_record::<SourceEventsFamily>(TurnEventKey {
                    owner: tail_id,
                    ordinal,
                })?;
                if event.turn_id() != tail_id
                    || event.sequence() != ordinal
                    || !matches!(event.payload(), crate::SourceEventPayload::TurnEnded(status) if state.source_end_status() == Some(*status))
                {
                    return Err(SyndicReadError::Invariant(
                        "provider context terminal source disagrees",
                    ));
                }
                let Some(source) = event.source() else {
                    return Ok(
                        if target.context_owner_id().is_some()
                            || origin.context_owner_id().is_some()
                        {
                            ContextPresence::Required
                        } else {
                            ContextPresence::Unclassified
                        },
                    );
                };
                let index = self.required_record::<CasTurnIndexFamily>(CasTurnKey::Record(
                    source.thread_id().clone(),
                    source.turn_id().clone(),
                ))?;
                let snapshot =
                    self.required_record::<ExecutionSnapshotsFamily>(index.snapshot_id())?;
                let binding = self.required_record::<BindingsFamily>(BindingKey {
                    thread: index.thread_id(),
                    revision: index.binding_revision(),
                })?;
                let usable = match binding.state() {
                    crate::BindingState::Valid(usable) => usable,
                    crate::BindingState::Active(active) => active.usable(),
                    _ => {
                        return Err(SyndicReadError::Invariant(
                            "provider context source binding not usable",
                        ));
                    }
                };
                if index.turn_id() != tail_id
                    || event.turn_id() != tail_id
                    || event.sequence() != ordinal
                    || !matches!(event.payload(), crate::SourceEventPayload::TurnEnded(status)
                        if state.source_end_status() == Some(*status))
                    || index.thread_id() != tail.origin_thread_id()
                    || snapshot.id() != index.snapshot_id()
                    || snapshot.thread_id() != index.thread_id()
                    || snapshot.binding_revision() != index.binding_revision()
                    || snapshot.active_turn_id() != tail_id
                    || snapshot.cas_thread_id() != source.thread_id()
                    || snapshot.lineage() != usable.lineage()
                    || snapshot.execution() != usable.execution()
                    || snapshot.tool_profile() != usable.tool_profile()
                    || snapshot.represented_base_prefix() != usable.represented_prefix()
                    || snapshot.represented_base_native_turn_count() != usable.native_turn_count()
                    || !matches!((tail.kind(), snapshot.kind()), (crate::TurnKind::ProviderOperation(a), crate::ExecutionSnapshotKind::ProviderOperation(b)) if a == b)
                {
                    return Err(SyndicReadError::Invariant(
                        "provider context presence original snapshot disagrees",
                    ));
                }
                Some(snapshot)
            }
            TurnDispatchProvenance::Unattempted => None,
        };
        Ok(
            if target.context_owner_id().is_some()
                || origin.context_owner_id().is_some()
                || snapshot.as_ref().is_some_and(|snapshot| {
                    matches!(snapshot.lineage(), CasLineageProof::ContextAware(_))
                })
            {
                ContextPresence::Required
            } else if snapshot.is_some() {
                ContextPresence::Absent
            } else {
                ContextPresence::Unclassified
            },
        )
    }
}
