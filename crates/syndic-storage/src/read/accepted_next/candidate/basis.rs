use super::*;

fn required<T>(record: Option<T>, message: &'static str) -> Result<T, SyndicReadError> {
    record.ok_or(SyndicReadError::Invariant(message))
}

impl SyndicStorage {
    pub(super) fn load_next_candidate_basis(
        &self,
        store: &HomeStore,
        source_revision: DomainRevision,
        snapshot: NextAuthoritySnapshot,
        order: AcceptedOrderIndexRecord,
        leaf: AcceptedRouteLeafRecord,
    ) -> Result<AcceptedNextCandidateBasis, SyndicReadError> {
        let limit = SyndicPointReadLimit::new(NEXT_POINT_MAX_BYTES)
            .expect("accepted-next point bound is nonzero");
        let source = required(
            snapshot.source,
            "accepted-next candidate source record is missing",
        )?;
        let gate = required(
            snapshot.gate,
            "accepted-next candidate input gate is missing",
        )?;
        let generation = required(
            snapshot.generation,
            "accepted-next candidate generation is missing",
        )?;
        let input = required(
            self.point::<AcceptedInputsFamily>(store, order.input_id(), limit)?,
            "accepted-next candidate input is missing",
        )?;
        let thread = required(
            self.point::<ThreadsFamily>(store, source.thread_id(), limit)?,
            "accepted-next candidate thread is missing",
        )?;
        let draft_by_thread = required(
            self.point::<DraftByThreadFamily>(store, source.thread_id(), limit)?,
            "accepted-next candidate draft reverse record is missing",
        )?;
        let binding_head = required(
            self.point::<BindingHeadsFamily>(store, source.thread_id(), limit)?,
            "accepted-next candidate binding head is missing",
        )?;
        let binding = required(
            self.point::<BindingsFamily>(
                store,
                BindingKey {
                    thread: source.thread_id(),
                    revision: binding_head.revision(),
                },
                limit,
            )?,
            "accepted-next candidate binding is missing",
        )?;
        let transcript_head = required(
            self.point::<TranscriptHeadsFamily>(store, source.thread_id(), limit)?,
            "accepted-next candidate transcript head is missing",
        )?;
        let summary = required(
            self.point::<HistorySummariesFamily>(store, source.thread_id(), limit)?,
            "accepted-next candidate history summary is missing",
        )?;
        let activity_head = required(
            self.point::<ActivityQueryHeadsFamily>(store, source.thread_id(), limit)?,
            "accepted-next candidate activity head is missing",
        )?;
        Ok(AcceptedNextCandidateBasis {
            source_revision,
            source,
            gate,
            thread,
            draft_by_thread,
            route_head: snapshot.head,
            generation,
            leaf,
            input,
            order,
            binding_head,
            binding,
            transcript_head,
            summary,
            activity_head,
        })
    }
}
