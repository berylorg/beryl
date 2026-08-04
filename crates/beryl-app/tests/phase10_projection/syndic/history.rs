use super::*;

impl Fixture {
    pub fn activate_without_terminal(&mut self, submitted: SubmittedTurn) -> CasTurnSource {
        let started_at = self.tick();
        let source = exact::establish_turn(
            &self.store,
            self.storage,
            self.thread,
            submitted.turn,
            started_at,
        );
        self.admit(
            self.thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnActivated,
        );
        let user_observed_at = self.tick();
        exact::correlate_user_item(
            &self.store,
            self.storage,
            self.thread,
            submitted.turn,
            submitted.user_item,
            &source,
            user_observed_at,
        );
        source
    }

    pub fn complete_active_without_assistant(
        &mut self,
        submitted: SubmittedTurn,
        source: &CasTurnSource,
    ) {
        self.admit(
            self.thread,
            submitted.turn,
            source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        );
        self.finalize_all(self.thread, submitted.turn);
    }

    pub fn mark_active_unknown_terminal(
        &mut self,
        submitted: SubmittedTurn,
        source: &CasTurnSource,
    ) {
        self.admit(
            self.thread,
            submitted.turn,
            source,
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(
                    TurnTerminalOutcome::UnknownTerminal,
                    Some(TurnIncompleteReason::ItemAuditFailed),
                )
                .unwrap(),
            ),
        );
    }

    pub fn complete_with_assistant(&mut self, submitted: SubmittedTurn, text: &str) {
        self.complete_with_assistant_on(self.thread, submitted, text);
    }

    pub fn complete_with_assistant_on(
        &mut self,
        thread: SyndicThreadId,
        submitted: SubmittedTurn,
        text: &str,
    ) {
        let started_at = self.tick();
        let source = exact::establish_turn(
            &self.store,
            self.storage,
            thread,
            submitted.turn,
            started_at,
        );
        self.admit(
            thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnActivated,
        );
        let user_observed_at = self.tick();
        exact::correlate_user_item(
            &self.store,
            self.storage,
            thread,
            submitted.turn,
            submitted.user_item,
            &source,
            user_observed_at,
        );
        let item = SyndicItemId::from_bytes([self.next_item; 16]);
        self.next_item = self.next_item.checked_add(1).unwrap();
        let cas_item = CasItemId::new(format!("phase10-item-{item}")).unwrap();
        let agent_value = |text: &str| {
            ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
                text: ProviderTextV1::inline(text),
                phase: Some(ProviderMessagePhaseV1::FinalAnswer),
                memory_citation: None,
            })
        };
        let assistant_started_at = self.tick();
        exact::admit_item_frame(
            &self.store,
            self.storage,
            thread,
            submitted.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::FIRST,
                cas_item.clone(),
                ProviderItemObservationV1::Started {
                    observed_at: ProviderLifecycleTimestampMsV1::new(
                        assistant_started_at.unix_millis(),
                    ),
                    item: agent_value(""),
                },
            ),
            assistant_started_at,
        );
        let assistant_delta_at = self.tick();
        exact::admit_item_frame(
            &self.store,
            self.storage,
            thread,
            submitted.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::new(2).unwrap(),
                cas_item.clone(),
                ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                    delta: ProviderTextV1::inline(text),
                }),
            ),
            assistant_delta_at,
        );
        let assistant_completed_at = self.tick();
        exact::admit_item_frame(
            &self.store,
            self.storage,
            thread,
            submitted.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::new(3).unwrap(),
                cas_item,
                ProviderItemObservationV1::Completed {
                    observed_at: ProviderLifecycleTimestampMsV1::new(
                        assistant_completed_at.unix_millis(),
                    ),
                    item: agent_value(text),
                },
            ),
            assistant_completed_at,
        );
        self.admit(
            thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        );
        self.finalize_all(thread, submitted.turn);
    }

    fn admit(
        &mut self,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        source: &CasTurnSource,
        payload: SourceEventPayload,
    ) {
        let observed_at = self.tick();
        exact::admit_event(
            &self.store,
            self.storage,
            thread,
            turn,
            source,
            payload,
            observed_at,
        );
    }

    fn finalize_all(&mut self, thread: SyndicThreadId, turn: SyndicTurnId) {
        let indexes = self
            .storage
            .turn_items(
                &self.store,
                turn,
                None,
                CursorReadLimits::new(64, 1_000_000).unwrap(),
            )
            .unwrap()
            .records()
            .to_vec();
        for index in indexes {
            let item = self
                .storage
                .canonical_item(&self.store, index.item_id(), point_limit())
                .unwrap()
                .unwrap();
            let content = item
                .provider_content()
                .or_else(|| item.presentation_content())
                .expect("finalization fixture item must own closed content");
            let manifest = self
                .storage
                .content_manifest(&self.store, content.id(), point_limit())
                .unwrap()
                .unwrap();
            if manifest.lifecycle() == ContentLifecycle::Live {
                let state = self
                    .storage
                    .turn_state(&self.store, turn, point_limit())
                    .unwrap()
                    .unwrap();
                let updated_at = self.tick();
                execute(
                    &self.store,
                    self.storage.freeze_next_turn_item(
                        self.storage.revision(&self.store).unwrap(),
                        FreezeNextTurnItem::new(
                            thread,
                            turn,
                            state.revision(),
                            index.ordinal(),
                            index.item_id(),
                            updated_at,
                        ),
                    ),
                );
            }
            let item = self
                .storage
                .canonical_item(&self.store, index.item_id(), point_limit())
                .unwrap()
                .unwrap();
            if matches!(
                item.kind(),
                CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
            ) {
                project_item(&self.store, self.storage, index.item_id());
            }
            let state = self
                .storage
                .turn_state(&self.store, turn, point_limit())
                .unwrap()
                .unwrap();
            let updated_at = self.tick();
            execute(
                &self.store,
                self.storage.finalize_next_turn_item(
                    self.storage.revision(&self.store).unwrap(),
                    FinalizeNextTurnItem::new(
                        thread,
                        turn,
                        state.revision(),
                        index.ordinal(),
                        index.item_id(),
                        updated_at,
                    ),
                ),
            );
        }
        self.finish_transcript(thread);
        self.release_terminal_history(thread, turn);
    }

    fn release_terminal_history(&self, thread: SyndicThreadId, turn: SyndicTurnId) {
        let gate = self
            .storage
            .input_gate(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let state = self
            .storage
            .turn_state(&self.store, turn, point_limit())
            .unwrap()
            .unwrap();
        let head = self
            .storage
            .transcript_view_head(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        self.store
            .execute_current(self.storage.current_complete_terminal_history(
                CompleteTerminalHistory::new(
                    thread,
                    turn,
                    gate,
                    state.revision(),
                    head.generation(),
                    head.revision(),
                ),
            ))
            .unwrap();
    }

    pub(super) fn finish_transcript(&self, thread: SyndicThreadId) {
        let thread_record = self
            .storage
            .thread(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let head = self
            .storage
            .transcript_view_head(&self.store, thread, point_limit())
            .unwrap()
            .unwrap();
        let generation = head.generation();
        execute(
            &self.store,
            self.storage.start_transcript_build(
                self.storage.revision(&self.store).unwrap(),
                StartTranscriptBuild::new(thread, thread_record.revision(), head.revision()),
            ),
        );
        for _ in 0..1_024 {
            let build = self
                .storage
                .transcript_build(&self.store, thread, generation, point_limit())
                .unwrap()
                .unwrap();
            if build.phase() == TranscriptBuildPhase::Complete {
                return;
            }
            execute(
                &self.store,
                self.storage.advance_transcript_build(
                    self.storage.revision(&self.store).unwrap(),
                    AdvanceTranscriptBuild::new(thread, generation, build.revision()),
                ),
            );
        }
        panic!("fixture transcript build did not converge")
    }
}
