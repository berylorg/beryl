use super::*;

impl Fixture {
    pub fn activate_without_terminal(&mut self, submitted: SubmittedTurn) -> CasTurnSource {
        let started_at = self.tick();
        let source = {
            let command_home = self.store.live_home_command().unwrap();
            exact::establish_turn(
                command_home.home(),
                self.storage,
                self.thread,
                submitted.turn,
                started_at,
            )
        };
        self.admit(
            self.thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnActivated,
        );
        let user_observed_at = self.tick();
        let command_home = self.store.live_home_command().unwrap();
        exact::correlate_user_item(
            command_home.home(),
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
        let source = {
            let command_home = self.store.live_home_command().unwrap();
            exact::establish_turn(
                command_home.home(),
                self.storage,
                thread,
                submitted.turn,
                started_at,
            )
        };
        self.admit(
            thread,
            submitted.turn,
            &source,
            SourceEventPayload::TurnActivated,
        );
        let user_observed_at = self.tick();
        {
            let command_home = self.store.live_home_command().unwrap();
            exact::correlate_user_item(
                command_home.home(),
                self.storage,
                thread,
                submitted.turn,
                submitted.user_item,
                &source,
                user_observed_at,
            );
        }
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
        {
            let command_home = self.store.live_home_command().unwrap();
            exact::admit_item_frame(
                command_home.home(),
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
        }
        let assistant_delta_at = self.tick();
        {
            let command_home = self.store.live_home_command().unwrap();
            exact::admit_item_frame(
                command_home.home(),
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
        }
        let assistant_completed_at = self.tick();
        {
            let command_home = self.store.live_home_command().unwrap();
            exact::admit_item_frame(
                command_home.home(),
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
        }
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
        let command_home = self.store.live_home_command().unwrap();
        exact::admit_event(
            command_home.home(),
            self.storage,
            thread,
            turn,
            source,
            payload,
            observed_at,
        );
    }

    fn finalize_all(&mut self, thread: SyndicThreadId, turn: SyndicTurnId) {
        let indexes = {
            let command_home = self.store.live_home_command().unwrap();
            self.storage
                .turn_items(
                    command_home.home(),
                    turn,
                    None,
                    CursorReadLimits::new(64, 1_000_000).unwrap(),
                )
                .unwrap()
                .records()
                .to_vec()
        };
        for index in indexes {
            let manifest = {
                let command_home = self.store.live_home_command().unwrap();
                let home = command_home.home();
                let item = self
                    .storage
                    .canonical_item(home, index.item_id(), point_limit())
                    .unwrap()
                    .unwrap();
                let content = item
                    .provider_content()
                    .or_else(|| item.presentation_content())
                    .expect("finalization fixture item must own closed content");
                self.storage
                    .content_manifest(home, content.id(), point_limit())
                    .unwrap()
                    .unwrap()
            };
            if manifest.lifecycle() == ContentLifecycle::Live {
                let state = {
                    let command_home = self.store.live_home_command().unwrap();
                    self.storage
                        .turn_state(command_home.home(), turn, point_limit())
                        .unwrap()
                        .unwrap()
                };
                let updated_at = self.tick();
                let command_home = self.store.live_home_command().unwrap();
                let home = command_home.home();
                execute(
                    home,
                    self.storage.freeze_next_turn_item(
                        self.storage.revision(home).unwrap(),
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
            let item = {
                let command_home = self.store.live_home_command().unwrap();
                self.storage
                    .canonical_item(command_home.home(), index.item_id(), point_limit())
                    .unwrap()
                    .unwrap()
            };
            if matches!(
                item.kind(),
                CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
            ) {
                let command_home = self.store.live_home_command().unwrap();
                project_item(command_home.home(), self.storage, index.item_id());
            }
            let state = {
                let command_home = self.store.live_home_command().unwrap();
                self.storage
                    .turn_state(command_home.home(), turn, point_limit())
                    .unwrap()
                    .unwrap()
            };
            let updated_at = self.tick();
            let command_home = self.store.live_home_command().unwrap();
            let home = command_home.home();
            execute(
                home,
                self.storage.finalize_next_turn_item(
                    self.storage.revision(home).unwrap(),
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
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        let gate = self
            .storage
            .input_gate(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let state = self
            .storage
            .turn_state(home, turn, point_limit())
            .unwrap()
            .unwrap();
        let head = self
            .storage
            .transcript_view_head(home, thread, point_limit())
            .unwrap()
            .unwrap();
        match home.execute_current(self.storage.current_complete_terminal_history(
            CompleteTerminalHistory::new(
                thread,
                turn,
                gate,
                state.revision(),
                head.generation(),
                head.revision(),
            ),
        )) {
            beryl_home_store::CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            beryl_home_store::CommandOutcome::NotCommitted { evidence } => {
                panic!("complete terminal history unexpectedly not committed: {evidence:?}")
            }
            outcome @ beryl_home_store::CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("complete terminal history committed with later failure: {outcome:?}"),
            outcome @ beryl_home_store::CommandOutcome::Indeterminate { .. } => {
                panic!("complete terminal history indeterminate: {outcome:?}")
            }
        }
    }

    pub(super) fn finish_transcript(&self, thread: SyndicThreadId) {
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        let thread_record = self
            .storage
            .thread(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let head = self
            .storage
            .transcript_view_head(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let generation = head.generation();
        execute(
            home,
            self.storage.start_transcript_build(
                self.storage.revision(home).unwrap(),
                StartTranscriptBuild::new(thread, thread_record.revision(), head.revision()),
            ),
        );
        for _ in 0..1_024 {
            let build = self
                .storage
                .transcript_build(home, thread, generation, point_limit())
                .unwrap()
                .unwrap();
            if build.phase() == TranscriptBuildPhase::Complete {
                return;
            }
            execute(
                home,
                self.storage.advance_transcript_build(
                    self.storage.revision(home).unwrap(),
                    AdvanceTranscriptBuild::new(thread, generation, build.revision()),
                ),
            );
        }
        panic!("fixture transcript build did not converge")
    }
}
