use super::*;

impl Builder<'_> {
    pub fn complete_with_operational(&mut self, submitted: SubmittedTurn, text: &str) {
        let source = self.establish_exact_cas(submitted.turn);
        self.admit_exact(submitted.turn, &source, SourceEventPayload::TurnActivated);
        self.correlate_user_input(submitted, &source);
        let item = self.allocate_item();
        let cas_item = CasItemId::new(format!("test-item-{item}")).unwrap();
        let started_at = self.tick();
        exact_cas::admit_item_frame(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::FIRST,
                cas_item.clone(),
                ProviderItemObservationV1::Started {
                    observed_at: ProviderLifecycleTimestampMsV1::new(started_at.unix_millis()),
                    item: command_item(ProviderCommandStatusV1::InProgress, None),
                },
            ),
            started_at,
        );
        let delta_at = self.tick();
        exact_cas::admit_item_frame(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::new(2).unwrap(),
                cas_item.clone(),
                ProviderItemObservationV1::Delta(ProviderItemDeltaV1::CommandExecutionOutput {
                    delta: ProviderTextV1::inline(text),
                }),
            ),
            delta_at,
        );
        let completed_at = self.tick();
        exact_cas::admit_item_frame(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::new(3).unwrap(),
                cas_item,
                ProviderItemObservationV1::Completed {
                    observed_at: ProviderLifecycleTimestampMsV1::new(completed_at.unix_millis()),
                    item: command_item(ProviderCommandStatusV1::Completed, Some(text)),
                },
            ),
            completed_at,
        );
        self.admit_exact(
            submitted.turn,
            &source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        );
        self.finalize_all(submitted.turn);
        terminal::release(self.store, self.storage, self.thread, submitted.turn);
    }

    pub(super) fn establish_exact_cas(&mut self, turn: SyndicTurnId) -> CasTurnSource {
        let started_at = self.tick();
        exact_cas::establish_turn(self.store, self.storage, self.thread, turn, started_at)
    }

    pub(super) fn admit_exact(
        &mut self,
        turn: SyndicTurnId,
        source: &CasTurnSource,
        payload: SourceEventPayload,
    ) {
        let observed_at = self.tick();
        exact_cas::admit_event(
            self.store,
            self.storage,
            self.thread,
            turn,
            source,
            payload,
            observed_at,
        );
    }

    pub(super) fn correlate_user_input(
        &mut self,
        submitted: SubmittedTurn,
        source: &CasTurnSource,
    ) {
        let observed_at = self.tick();
        exact_cas::correlate_user_item(
            self.store,
            self.storage,
            self.thread,
            submitted.turn,
            submitted.user_item,
            source,
            observed_at,
        );
    }

    pub(super) fn allocate_item(&mut self) -> SyndicItemId {
        let item = SyndicItemId::from_bytes([self.next_item; 16]);
        self.next_item = self.next_item.checked_add(1).unwrap();
        item
    }
}
