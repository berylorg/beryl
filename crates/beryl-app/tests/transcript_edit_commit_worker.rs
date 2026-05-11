#![allow(dead_code, private_interfaces, unused_imports)]

pub use beryl_app::{
    BerylWorkspacePersistence, WorkspaceImageAsset, WorkspaceImageAssetStatus,
    WorkspacePersistenceError,
};

mod shell {
    use std::{path::PathBuf, time::Duration};

    use beryl_backend::{ThreadInfo, ThreadRollbackResponse, ThreadSummary, TurnStartOptions};
    use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
    use serde_json::json;

    #[path = "../../src/shell/composer_draft.rs"]
    mod composer_draft;
    #[path = "../../src/shell/composer_image_delivery.rs"]
    mod composer_image_delivery;
    #[path = "../../src/shell/composer_image_labels.rs"]
    mod composer_image_labels;
    #[path = "../../src/shell/composer_submission.rs"]
    mod composer_submission;
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_edit_commit_worker.rs"]
    mod transcript_edit_commit_worker;
    #[path = "../../src/shell/transcript_edit_menu_state.rs"]
    mod transcript_edit_menu_state;
    #[path = "../../src/shell/transcript_image_sources.rs"]
    mod transcript_image_sources;
    #[path = "../../src/shell/transcript_presentation.rs"]
    mod transcript_presentation;
    #[path = "../../src/shell/transcript_projection.rs"]
    mod transcript_projection;
    #[path = "../../src/shell/transcript_stream_invalidation.rs"]
    mod transcript_stream_invalidation;
    #[allow(dead_code)]
    #[path = "../../src/shell/virtual_list/mod.rs"]
    mod virtual_list;

    use composer_draft::{AcceptedComposerDraft, ComposerDraft};
    use execution_detail::UserInputFragment;
    use transcript_edit_commit_worker::{
        TranscriptEditCommitOutcome, TranscriptEditCommitRequest, TranscriptEditRollbackBackend,
        run_transcript_edit_rollback,
    };
    use transcript_edit_menu_state::TranscriptEditTarget;
    use transcript_stream_invalidation::TranscriptStreamInvalidations;

    #[test]
    fn edit_commit_passes_exact_rollback_count_to_backend() {
        for (source_turn_id, source_turn_index, rollback_count) in [
            ("turn_1", 0usize, 3u32),
            ("turn_2", 1usize, 2u32),
            ("turn_3", 2usize, 1u32),
        ] {
            let mut backend = FakeEditBackend::new(
                ["turn_1", "turn_2", "turn_3"],
                Ok(rollback_response(thread_info(&["turn_1", "turn_2"]))),
            );
            let request = request(source_turn_id, source_turn_index, rollback_count);

            let outcome = run_transcript_edit_rollback(
                &mut backend,
                request,
                UserInputFragment::text("Replacement prompt"),
                Duration::from_secs(1),
            );

            assert_eq!(
                backend.rollback_calls,
                vec![("source_thread".to_string(), rollback_count)]
            );
            assert!(matches!(
                outcome,
                TranscriptEditCommitOutcome::RolledBack { .. }
            ));
            assert_eq!(
                backend.visible_turns.len(),
                3usize.saturating_sub(rollback_count as usize)
            );
        }
    }

    #[test]
    fn edit_commit_reports_rollback_failure_without_backend_mutation_fallback() {
        let mut backend = FakeEditBackend::new(
            ["turn_1", "turn_2", "turn_3"],
            Err("rollback rejected".to_string()),
        );
        let outcome = run_transcript_edit_rollback(
            &mut backend,
            request("turn_2", 1, 2),
            UserInputFragment::text("Replacement prompt"),
            Duration::from_secs(1),
        );

        match outcome {
            TranscriptEditCommitOutcome::RollbackFailed { request, message } => {
                assert_eq!(request.source_thread_id(), "source_thread");
                assert_eq!(
                    request.accepted_draft().display_text(),
                    "Replacement prompt"
                );
                assert!(message.contains("rollback rejected"));
            }
            TranscriptEditCommitOutcome::RolledBack { .. }
            | TranscriptEditCommitOutcome::PreRollbackFailed { .. } => {
                panic!("expected rollback failure")
            }
        }
        assert_eq!(
            backend.rollback_calls,
            vec![("source_thread".to_string(), 2)]
        );
        assert_eq!(
            backend.visible_turns,
            vec![
                "turn_1".to_string(),
                "turn_2".to_string(),
                "turn_3".to_string()
            ]
        );
    }

    #[test]
    fn rollback_success_flow_resets_history_invalidates_tail_and_starts_replacement() {
        let mut backend = FakeEditBackend::new(
            ["turn_1", "turn_2", "turn_3"],
            Ok(rollback_response(thread_info_with_prompts(&[(
                "turn_1",
                "Kept prompt",
            )]))),
        );
        let request =
            request_with_discarded("turn_2", 1, 2, ["turn_2".to_string(), "turn_3".to_string()]);
        let outcome = run_transcript_edit_rollback(
            &mut backend,
            request,
            UserInputFragment::text("Replacement prompt"),
            Duration::from_secs(1),
        );
        let TranscriptEditCommitOutcome::RolledBack {
            request,
            thread,
            replacement_fragment,
            ..
        } = outcome
        else {
            panic!("expected successful rollback");
        };
        let mut flow = EditFlowHarness::default();

        flow.apply_rolled_back_thread_and_begin_replacement(
            &request,
            &thread,
            replacement_fragment,
        );

        assert_eq!(
            backend.rollback_calls,
            vec![("source_thread".to_string(), 2)]
        );
        assert_eq!(backend.visible_turns, vec!["turn_1".to_string()]);
        assert_eq!(
            flow.user_input_texts(),
            vec!["Kept prompt".to_string(), "Replacement prompt".to_string()]
        );
        assert_eq!(flow.replacement_thread_id(), Some("source_thread"));
        assert!(flow.event_targets_invalidated_tail("turn_2"));
        assert!(flow.event_targets_invalidated_tail("turn_3"));
        assert!(!flow.event_targets_invalidated_tail("turn_1"));
    }

    #[test]
    fn rollback_success_start_failure_keeps_tail_deleted_and_marks_replacement_failed() {
        let mut backend = FakeEditBackend::new(
            ["turn_1", "turn_2"],
            Ok(rollback_response(thread_info_with_prompts(&[(
                "turn_1",
                "Kept prompt",
            )]))),
        );
        let request = request_with_discarded("turn_2", 1, 1, ["turn_2".to_string()]);
        let outcome = run_transcript_edit_rollback(
            &mut backend,
            request,
            UserInputFragment::text("Replacement prompt"),
            Duration::from_secs(1),
        );
        let TranscriptEditCommitOutcome::RolledBack {
            request,
            thread,
            replacement_fragment,
            ..
        } = outcome
        else {
            panic!("expected successful rollback");
        };
        let mut flow = EditFlowHarness::default();
        flow.apply_rolled_back_thread_and_begin_replacement(
            &request,
            &thread,
            replacement_fragment,
        );

        flow.fail_replacement_before_turn_start("turn/start rejected");

        assert_eq!(backend.visible_turns, vec!["turn_1".to_string()]);
        assert_eq!(
            flow.user_input_texts(),
            vec!["Kept prompt".to_string(), "Replacement prompt".to_string()]
        );
        assert_eq!(flow.replacement_error(), Some("turn/start rejected"));
        assert!(flow.event_targets_invalidated_tail("turn_2"));
    }

    #[test]
    fn edit_replacement_late_binds_developer_instructions_at_turn_start() {
        let edit_mode_source = include_str!("../src/shell/transcript_edit_mode.rs");
        let edit_commit_source = include_str!("../src/shell/transcript_edit_commit.rs");

        let request_body =
            rust_function_body(edit_mode_source, "fn transcript_edit_commit_request");
        assert!(
            !request_body.contains("turn_options_with_current_developer_instructions"),
            "edit commit requests must not freeze developer instructions before rollback"
        );

        let finish_body = rust_function_body(
            edit_commit_source,
            "pub(super) fn finish_successful_transcript_edit_rollback",
        );
        let defaults_index = finish_body
            .find("effective_turn_context_defaults")
            .expect("successful rollback should preserve effective turn defaults before reload");
        let load_index = finish_body
            .find("load_thread_history_window")
            .expect("successful rollback should reload the rolled-back history");
        let replacement_index = finish_body
            .find("begin_transcript_edit_replacement_turn")
            .expect("successful rollback should start the replacement turn");
        assert!(
            defaults_index < load_index && load_index < replacement_index,
            "replacement defaults must be captured before history reload clears session metadata"
        );

        let replacement_body = rust_function_body(
            edit_commit_source,
            "fn begin_transcript_edit_replacement_turn",
        );
        let late_bind_index = replacement_body
            .find("turn_options_with_current_developer_instructions_defaults")
            .expect("replacement turn start should late-bind developer instructions");
        let worker_index = replacement_body
            .find("spawn_turn_worker")
            .expect("replacement turn start should spawn the replacement turn worker");
        assert!(
            replacement_body.contains("turn_context_defaults"),
            "replacement turn start should use the defaults captured before history reload"
        );
        assert!(
            late_bind_index < worker_index,
            "replacement turn start must attach developer instructions immediately before spawning the turn"
        );
    }

    struct FakeEditBackend {
        rollback_response: Option<Result<ThreadRollbackResponse, String>>,
        rollback_calls: Vec<(String, u32)>,
        visible_turns: Vec<String>,
    }

    #[derive(Default)]
    struct EditFlowHarness {
        details: execution_detail::ExecutionDetailState,
        presentation: transcript_presentation::TranscriptPresentationState,
        invalidations: TranscriptStreamInvalidations,
    }

    impl EditFlowHarness {
        fn apply_rolled_back_thread_and_begin_replacement(
            &mut self,
            request: &TranscriptEditCommitRequest,
            thread: &ThreadInfo,
            replacement_fragment: UserInputFragment,
        ) {
            self.details.load_thread_history(thread);
            self.presentation.replace_from_turns(self.details.turns());
            self.invalidations.invalidate_turns(
                request.source_thread_id(),
                request.discarded_turn_ids().iter().cloned(),
            );
            let turn_index = self.details.begin_turn_with_thread_fragments(
                Some(request.source_thread_id().to_string()),
                vec![replacement_fragment],
            );
            let turn = self.details.turns()[turn_index].clone();
            self.presentation
                .append_turn(turn_index, turn)
                .expect("replacement prompt should be visible");
        }

        fn fail_replacement_before_turn_start(&mut self, message: &str) {
            self.details.finish_turn_failure(message.to_string());
            let turn_index = self.details.turns().len().saturating_sub(1);
            let turn = self.details.turns()[turn_index].clone();
            self.presentation
                .replace_turn(turn_index, turn)
                .expect("failed replacement prompt should stay visible");
        }

        fn user_input_texts(&self) -> Vec<String> {
            (0..self.presentation.len())
                .filter_map(|index| self.presentation.turn_at(index))
                .filter_map(|row| {
                    row.turn
                        .user_input_fragments()
                        .first()
                        .map(|fragment| fragment.text.clone())
                })
                .collect()
        }

        fn replacement_thread_id(&self) -> Option<&str> {
            self.details
                .turns()
                .last()
                .and_then(|turn| turn.thread_id.as_deref())
        }

        fn replacement_error(&self) -> Option<&str> {
            self.details
                .turns()
                .last()
                .and_then(|turn| turn.error_message.as_deref())
        }

        fn event_targets_invalidated_tail(&self, turn_id: &str) -> bool {
            self.invalidations.event_targets_invalidated_turn(
                &beryl_backend::TurnStreamEvent::AgentMessageDelta {
                    thread_id: "source_thread".to_string(),
                    turn_id: turn_id.to_string(),
                    item_id: "message_1".to_string(),
                    delta: "late output".to_string(),
                },
            )
        }
    }

    impl FakeEditBackend {
        fn new(
            visible_turns: impl IntoIterator<Item = &'static str>,
            rollback_response: Result<ThreadRollbackResponse, String>,
        ) -> Self {
            Self {
                rollback_response: Some(rollback_response),
                rollback_calls: Vec::new(),
                visible_turns: visible_turns.into_iter().map(str::to_string).collect(),
            }
        }
    }

    impl TranscriptEditRollbackBackend for FakeEditBackend {
        type Error = String;

        fn rollback_thread(
            &mut self,
            thread_id: &str,
            num_turns: u32,
            _: Duration,
        ) -> Result<ThreadRollbackResponse, Self::Error> {
            self.rollback_calls.push((thread_id.to_string(), num_turns));
            let response = self
                .rollback_response
                .take()
                .expect("rollback should only be called once");
            if response.is_ok() {
                let keep = self.visible_turns.len().saturating_sub(num_turns as usize);
                self.visible_turns.truncate(keep);
            }
            response
        }
    }

    fn request(
        source_turn_id: &str,
        source_turn_index: usize,
        rollback_count: u32,
    ) -> TranscriptEditCommitRequest {
        request_with_discarded(
            source_turn_id,
            source_turn_index,
            rollback_count,
            [source_turn_id.to_string()],
        )
    }

    fn request_with_discarded(
        source_turn_id: &str,
        source_turn_index: usize,
        rollback_count: u32,
        discarded_turn_ids: impl IntoIterator<Item = String>,
    ) -> TranscriptEditCommitRequest {
        TranscriptEditCommitRequest::new(
            BerylWorkspaceId::untitled(1),
            WorkspaceId::host_windows(r"C:\work\alpha"),
            TranscriptEditTarget::for_test(
                "source_thread",
                source_turn_id,
                source_turn_index,
                rollback_count,
                vec!["Original prompt".to_string()],
            ),
            discarded_turn_ids.into_iter().collect(),
            composer_draft_for_test("Replacement prompt"),
            true,
            TurnStartOptions::default(),
        )
    }

    fn composer_draft_for_test(text: &str) -> AcceptedComposerDraft {
        let mut draft = ComposerDraft::default();
        draft.sync_display_text(text);
        draft.accepted().expect("draft should be accepted")
    }

    fn rollback_response(thread: ThreadInfo) -> ThreadRollbackResponse {
        ThreadRollbackResponse { thread }
    }

    fn thread_info(turn_ids: &[&str]) -> ThreadInfo {
        serde_json::from_value(json!({
            "createdAt": 10,
            "cwd": r"C:\work\alpha",
            "ephemeral": false,
            "id": "source_thread",
            "modelProvider": "openai",
            "preview": "Source preview",
            "source": "appServer",
            "status": { "type": "idle" },
            "turns": turn_ids.iter().map(|turn_id| {
                json!({
                    "id": turn_id,
                    "status": "completed",
                    "items": []
                })
            }).collect::<Vec<_>>(),
            "updatedAt": 20
        }))
        .unwrap()
    }

    fn thread_info_with_prompts(turns: &[(&str, &str)]) -> ThreadInfo {
        serde_json::from_value(json!({
            "createdAt": 10,
            "cwd": r"C:\work\alpha",
            "ephemeral": false,
            "id": "source_thread",
            "modelProvider": "openai",
            "preview": "Source preview",
            "source": "appServer",
            "status": { "type": "idle" },
            "turns": turns.iter().map(|(turn_id, prompt)| {
                json!({
                    "id": turn_id,
                    "status": "completed",
                    "items": [{
                        "id": format!("{turn_id}_user"),
                        "type": "userMessage",
                        "content": [{
                            "type": "text",
                            "text": prompt
                        }]
                    }]
                })
            }).collect::<Vec<_>>(),
            "updatedAt": 20
        }))
        .unwrap()
    }

    fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
        let signature_index = source
            .find(function_signature)
            .unwrap_or_else(|| panic!("missing shell function {function_signature}"));
        let after_signature = &source[signature_index..];
        let open_offset = after_signature
            .find('{')
            .unwrap_or_else(|| panic!("missing body for {function_signature}"));
        let body_start = signature_index + open_offset;
        let mut depth = 0usize;

        for (offset, character) in source[body_start..].char_indices() {
            match character {
                '{' => depth = depth.saturating_add(1),
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return &source[body_start..body_start + offset + character.len_utf8()];
                    }
                }
                _ => {}
            }
        }

        panic!("unterminated body for {function_signature}");
    }
}
