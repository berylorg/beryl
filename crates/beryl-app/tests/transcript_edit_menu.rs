#![allow(dead_code, private_interfaces, unused_imports)]

use beryl_backend::{
    AgentMessageItem, CommandExecutionItem, CommandExecutionStatus, ProtocolPhase,
    ThreadBranchCapabilities, ThreadItem, TurnInfo, TurnStatus, UserInput, UserMessageItem,
};

mod shell {
    #[path = "../../src/shell/composer_draft.rs"]
    mod composer_draft;
    #[path = "../../src/shell/composer_image_labels.rs"]
    mod composer_image_labels;
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_edit_menu_state.rs"]
    pub(super) mod transcript_edit_menu_state;
    #[path = "../../src/shell/transcript_presentation.rs"]
    mod transcript_presentation;
    #[path = "../../src/shell/transcript_projection.rs"]
    mod transcript_projection;
    #[allow(dead_code)]
    #[path = "../../src/shell/virtual_list/mod.rs"]
    mod virtual_list;

    use std::ops::Range;

    use beryl_backend::{TurnInfo, TurnStreamEvent};

    pub(super) use self::transcript_edit_menu_state::{
        EDIT_COMPOSER_NOT_EMPTY_TOOLTIP, TranscriptEditDisabledReason, TranscriptEditMenuEntry,
        TranscriptEditMenuGate, TranscriptEditTarget, TranscriptEditTargetResolution,
        transcript_edit_menu_entry,
    };
    pub(super) use self::{
        composer_draft::AcceptedComposerDraftPart,
        composer_image_labels::ComposerImagePasteReadiness,
        execution_detail::{
            TranscriptImagePathResolver, TranscriptImagePreviewState,
            TranscriptImageSourceResolution,
        },
    };
    use self::{
        execution_detail::ExecutionDetailState,
        transcript_presentation::TranscriptPresentationState,
    };

    pub(super) struct EditHarness {
        details: ExecutionDetailState,
        presentation: TranscriptPresentationState,
    }

    impl EditHarness {
        pub(super) fn new() -> Self {
            Self {
                details: ExecutionDetailState::default(),
                presentation: TranscriptPresentationState::default(),
            }
        }

        pub(super) fn replace_history(&mut self, thread_id: &str, turns: Vec<TurnInfo>) {
            self.details = ExecutionDetailState::default();
            self.details.prepend_thread_history_page(thread_id, turns);
            self.presentation.replace_from_turns(self.details.turns());
        }

        pub(super) fn replace_history_with_image_resolver(
            &mut self,
            thread_id: &str,
            turns: Vec<TurnInfo>,
            image_resolver: &TranscriptImagePathResolver,
        ) {
            self.details = ExecutionDetailState::default();
            self.details
                .prepend_thread_history_page_with_image_resolver(thread_id, turns, image_resolver);
            self.presentation.replace_from_turns(self.details.turns());
        }

        pub(super) fn begin_live_turn(&mut self, text: &str) {
            let index = self.details.begin_turn(text.to_string());
            let turn = self.details.turns()[index].clone();
            self.presentation
                .append_turn(index, turn)
                .expect("live prompt should project into transcript");
        }

        pub(super) fn materialize_live_turn(&mut self, thread_id: &str, turn_id: &str) {
            let index = self
                .details
                .apply_stream_event(TurnStreamEvent::TurnStarted {
                    thread_id: thread_id.to_string(),
                    turn: TurnInfo {
                        id: turn_id.to_string(),
                        status: beryl_backend::TurnStatus::InProgress,
                        items: Vec::new(),
                        error: None,
                    },
                })
                .expect("live turn should accept turn start");
            let turn = self.details.turns()[index].clone();
            self.presentation.replace_turn(index, turn);
        }

        pub(super) fn release_range(&mut self, range: Range<usize>) {
            for replacement in self.details.release_history_range(range) {
                self.presentation.replace_turn_with_placeholder(
                    replacement.index,
                    replacement.turn,
                    None,
                );
            }
        }

        pub(super) fn target_at(
            &self,
            index: usize,
            current_tail_known: bool,
        ) -> Option<TranscriptEditTarget> {
            self.presentation.turn_at(index).and_then(|row| {
                TranscriptEditTarget::from_presented_row(
                    &row,
                    self.details.turns(),
                    current_tail_known,
                )
            })
        }

        pub(super) fn target_resolution_at(
            &self,
            index: usize,
            current_tail_known: bool,
        ) -> Option<TranscriptEditTargetResolution> {
            self.presentation.turn_at(index).and_then(|row| {
                TranscriptEditTarget::resolve_from_presented_row(
                    &row,
                    self.details.turns(),
                    current_tail_known,
                )
            })
        }

        pub(super) fn presentation_len(&self) -> usize {
            self.presentation.len()
        }
    }
}

use gpui::ImageFormat;
use shell::{
    AcceptedComposerDraftPart, ComposerImagePasteReadiness, EDIT_COMPOSER_NOT_EMPTY_TOOLTIP,
    EditHarness, TranscriptEditDisabledReason, TranscriptEditMenuEntry, TranscriptEditMenuGate,
    TranscriptEditTargetResolution, TranscriptImagePathResolver, TranscriptImagePreviewState,
    TranscriptImageSourceResolution, transcript_edit_menu_entry,
};

#[test]
fn edit_target_extracts_thread_turn_user_input_and_rollback_count() {
    let mut harness = EditHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn_with_fragments("turn_1", &["First fragment", "Second fragment"]),
            assistant_narrative_turn("turn_2", "Middle prompt", "Middle answer"),
            prompt_turn_with_fragments("turn_3", &["Latest fragment"]),
        ],
    );

    let first = harness.target_at(0, true).expect("first turn is editable");
    assert_eq!(first.source_thread_id(), "thread_a");
    assert_eq!(first.source_turn_id(), "turn_1");
    assert_eq!(first.source_turn_index(), 0);
    assert_eq!(first.rollback_turn_count(), 3);
    assert_eq!(
        first.draft_seed_fragments(),
        &["First fragment".to_string(), "Second fragment".to_string()]
    );
    assert_eq!(first.draft_seed_text(), "First fragment\n\nSecond fragment");

    let middle = harness
        .target_at(1, true)
        .expect("assistant narrative row still targets its parent turn");
    assert_eq!(middle.source_turn_id(), "turn_2");
    assert_eq!(middle.source_turn_index(), 1);
    assert_eq!(middle.rollback_turn_count(), 2);

    let latest = harness.target_at(2, true).expect("latest turn is editable");
    assert_eq!(latest.source_turn_id(), "turn_3");
    assert_eq!(latest.rollback_turn_count(), 1);
}

#[test]
fn edit_target_counts_hidden_operational_tail_turns() {
    let mut harness = EditHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn_with_fragments("turn_1", &["Prompt"]),
            operational_only_turn("turn_2"),
            prompt_turn_with_fragments("turn_3", &["Later prompt"]),
        ],
    );

    assert_eq!(harness.presentation_len(), 2);
    let first = harness
        .target_at(0, true)
        .expect("first prompt is editable");
    assert_eq!(first.rollback_turn_count(), 3);
    let latest_visible = harness
        .target_at(1, true)
        .expect("latest visible prompt is editable");
    assert_eq!(latest_visible.source_turn_id(), "turn_3");
    assert_eq!(latest_visible.rollback_turn_count(), 1);
}

#[test]
fn edit_target_rejects_non_backend_or_incomplete_history_rows() {
    let mut harness = EditHarness::new();
    harness.begin_live_turn("Unstarted prompt");
    assert!(harness.target_at(0, true).is_none());

    harness.materialize_live_turn("thread_a", "turn_live");
    assert!(harness.target_at(0, false).is_none());
}

#[test]
fn edit_target_rejects_placeholders_operational_rows_and_missing_user_input() {
    let mut harness = EditHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn_with_fragments("turn_1", &["Prompt"]),
            prompt_turn_with_fragments("turn_2", &["Later prompt"]),
        ],
    );
    harness.release_range(0..1);
    assert!(harness.target_at(0, true).is_none());

    harness.replace_history("thread_a", vec![operational_only_turn("turn_op")]);
    assert_eq!(harness.presentation_len(), 0);
    assert!(harness.target_at(0, true).is_none());

    harness.replace_history("thread_a", vec![assistant_only_turn("turn_assistant")]);
    assert_eq!(harness.presentation_len(), 1);
    assert!(harness.target_at(0, true).is_none());
}

#[test]
fn edit_target_disables_unreconstructable_inputs_with_clear_reasons() {
    let mut harness = EditHarness::new();
    harness.replace_history("thread_a", vec![local_image_turn("turn_image")]);
    assert_eq!(
        disabled_reason_at(&harness, 0),
        Some(TranscriptEditDisabledReason::MissingImageMetadata)
    );

    harness.replace_history("thread_a", vec![mention_turn("turn_mention")]);
    assert_eq!(
        disabled_reason_at(&harness, 0),
        Some(TranscriptEditDisabledReason::UnsupportedInput)
    );
}

#[test]
fn edit_target_reconstructs_image_only_and_mixed_image_turns() {
    let mut harness = EditHarness::new();
    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "C:\\image-a.png",
        TranscriptImageSourceResolution::available_asset_with_format(
            "asset_a",
            ImageFormat::Png,
            true,
        ),
    );
    resolver.insert_local_path_resolution(
        "C:\\image-b.png",
        TranscriptImageSourceResolution::available_asset_with_format(
            "asset_b",
            ImageFormat::Png,
            true,
        ),
    );

    harness.replace_history_with_image_resolver(
        "thread_a",
        vec![
            labeled_local_image_turn("turn_image_only", "A", "C:\\image-a.png"),
            mixed_local_image_turn("turn_mixed", "B", "C:\\image-b.png"),
        ],
        &resolver,
    );

    let image_only = harness
        .target_at(0, true)
        .expect("image-only turn should be editable");
    assert_eq!(image_only.draft_seed_text(), "[A]");
    assert_eq!(image_only.draft_seed().image_asset_ids(), vec!["asset_a"]);
    assert!(matches!(
        image_only.draft_seed().parts(),
        [AcceptedComposerDraftPart::Image(_)]
    ));

    let mixed = harness
        .target_at(1, true)
        .expect("mixed image/text turn should be editable");
    assert_eq!(mixed.draft_seed_text(), "before [B] after");
    assert_eq!(mixed.draft_seed().image_asset_ids(), vec!["asset_b"]);
    assert!(matches!(
        mixed.draft_seed().parts(),
        [
            AcceptedComposerDraftPart::Text(prefix),
            AcceptedComposerDraftPart::Image(_),
            AcceptedComposerDraftPart::Text(suffix),
        ] if prefix == "before " && suffix == " after"
    ));
}

#[test]
fn edit_target_disables_image_turns_with_missing_or_stale_asset_metadata() {
    for (resolution, expected_reason) in [
        (None, TranscriptEditDisabledReason::MissingImageMetadata),
        (
            Some(TranscriptImageSourceResolution::available_asset("asset_a")),
            TranscriptEditDisabledReason::MissingImageMetadata,
        ),
        (
            Some(TranscriptImageSourceResolution::unavailable_asset(
                "asset_a",
            )),
            TranscriptEditDisabledReason::MissingImageBytes,
        ),
        (
            Some(
                TranscriptImageSourceResolution::available_asset_with_format(
                    "asset_a",
                    ImageFormat::Png,
                    false,
                ),
            ),
            TranscriptEditDisabledReason::StaleImageRuntimePath,
        ),
    ] {
        let mut harness = EditHarness::new();
        let mut resolver = TranscriptImagePathResolver::default();
        if let Some(resolution) = resolution {
            resolver.insert_local_path_resolution("C:\\image.png", resolution);
        }
        harness.replace_history_with_image_resolver(
            "thread_a",
            vec![labeled_local_image_turn("turn_image", "A", "C:\\image.png")],
            &resolver,
        );

        assert_eq!(disabled_reason_at(&harness, 0), Some(expected_reason));
    }
}

#[test]
fn edit_target_preserves_multi_fragment_text_and_image_order() {
    let mut harness = EditHarness::new();
    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "C:\\image.png",
        TranscriptImageSourceResolution::available_asset_with_format(
            "asset_a",
            ImageFormat::Png,
            true,
        ),
    );
    harness.replace_history_with_image_resolver(
        "thread_a",
        vec![multi_fragment_text_and_image_turn(
            "turn_multi",
            "A",
            "C:\\image.png",
        )],
        &resolver,
    );

    let target = harness
        .target_at(0, true)
        .expect("multi-fragment target should reconstruct deterministically");
    assert_eq!(target.draft_seed_text(), "first\n\nlook [A]\n\nlast");
    assert_eq!(target.draft_seed().image_asset_ids(), vec!["asset_a"]);
    assert!(matches!(
        target.draft_seed().parts(),
        [
            AcceptedComposerDraftPart::Text(prefix),
            AcceptedComposerDraftPart::Image(_),
            AcceptedComposerDraftPart::Text(suffix),
        ] if prefix == "first\n\nlook " && suffix == "\n\nlast"
    ));
}

#[test]
fn edit_menu_gate_distinguishes_disabled_composer_from_unavailable_context() {
    let mut harness = EditHarness::new();
    harness.replace_history(
        "thread_a",
        vec![prompt_turn_with_fragments("turn_1", &["Prompt"])],
    );
    let target = harness
        .target_at(0, true)
        .expect("target should be editable");
    let allowed = allowed_gate();

    assert_eq!(
        transcript_edit_menu_entry(
            TranscriptEditTargetResolution::Enabled(target.clone()),
            allowed.clone()
        ),
        Some(TranscriptEditMenuEntry::Enabled(target.clone()))
    );
    let disabled = transcript_edit_menu_entry(
        TranscriptEditTargetResolution::Enabled(target.clone()),
        TranscriptEditMenuGate {
            composer_empty: false,
            ..allowed.clone()
        },
    )
    .expect("composer-filled edit row should still render disabled");
    assert_eq!(
        disabled.disabled_reason(),
        Some(TranscriptEditDisabledReason::ComposerNotEmpty)
    );

    assert_unavailable(
        target.clone(),
        TranscriptEditMenuGate {
            transcript_selection_active: true,
            ..allowed.clone()
        },
    );
    assert_unavailable(
        target.clone(),
        TranscriptEditMenuGate {
            source_thread_idle: false,
            ..allowed.clone()
        },
    );
    assert_unavailable(
        target.clone(),
        TranscriptEditMenuGate {
            selected_thread_matches_target: false,
            ..allowed.clone()
        },
    );
    assert_unavailable(
        target.clone(),
        TranscriptEditMenuGate {
            selected_thread_compaction_active: true,
            ..allowed.clone()
        },
    );
    assert_unavailable(
        target.clone(),
        TranscriptEditMenuGate {
            pending_thread_activation: true,
            ..allowed.clone()
        },
    );
    assert_unavailable(
        target.clone(),
        TranscriptEditMenuGate {
            rollback_capability_available: false,
            ..allowed.clone()
        },
    );
    assert_unavailable(
        target.clone(),
        TranscriptEditMenuGate {
            pending_turn_input: true,
            ..allowed.clone()
        },
    );
    assert_unavailable(
        target.clone(),
        TranscriptEditMenuGate {
            pending_active_turn_steering: true,
            ..allowed.clone()
        },
    );
    assert_unavailable(
        target,
        TranscriptEditMenuGate {
            conflicting_selected_thread_work: true,
            ..allowed
        },
    );
}

#[test]
fn image_edit_requires_completed_prior_label_scan() {
    let mut harness = EditHarness::new();
    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "C:\\image.png",
        TranscriptImageSourceResolution::available_asset_with_format(
            "asset_a",
            ImageFormat::Png,
            true,
        ),
    );
    harness.replace_history_with_image_resolver(
        "thread_a",
        vec![labeled_local_image_turn("turn_image", "A", "C:\\image.png")],
        &resolver,
    );
    let target = match harness
        .target_resolution_at(0, true)
        .expect("row should produce edit target resolution")
    {
        TranscriptEditTargetResolution::Enabled(target) => target,
        TranscriptEditTargetResolution::Disabled { reason, .. } => {
            panic!("image target should reconstruct before scan gate: {reason:?}")
        }
    };

    let scanning = transcript_edit_menu_entry(
        TranscriptEditTargetResolution::Enabled(target.clone()),
        TranscriptEditMenuGate {
            image_label_readiness: ComposerImagePasteReadiness::Scanning,
            ..allowed_gate()
        },
    )
    .expect("image edit row should render disabled while scanning");
    assert_eq!(
        scanning.disabled_reason(),
        Some(TranscriptEditDisabledReason::ImageLabelScanIncomplete)
    );

    let failed = transcript_edit_menu_entry(
        TranscriptEditTargetResolution::Enabled(target.clone()),
        TranscriptEditMenuGate {
            image_label_readiness: ComposerImagePasteReadiness::Failed {
                message: "scan failed".to_string(),
            },
            ..allowed_gate()
        },
    )
    .expect("image edit row should render disabled after scan failure");
    assert_eq!(
        failed.disabled_reason(),
        Some(TranscriptEditDisabledReason::ImageLabelScanFailed)
    );

    assert!(matches!(
        transcript_edit_menu_entry(
            TranscriptEditTargetResolution::Enabled(target),
            TranscriptEditMenuGate {
                image_label_readiness: ComposerImagePasteReadiness::Ready,
                ..allowed_gate()
            },
        ),
        Some(TranscriptEditMenuEntry::Enabled(_))
    ));
}

#[test]
fn edit_uses_rollback_capability_without_requiring_fork() {
    let rollback_only = ThreadBranchCapabilities::new(false, true);
    let fork_only = ThreadBranchCapabilities::new(true, false);

    assert!(rollback_only.thread_rollback());
    assert!(!rollback_only.thread_branching());
    assert!(!fork_only.thread_rollback());
}

#[test]
fn edit_composer_disabled_tooltip_text_is_stable() {
    assert_eq!(
        EDIT_COMPOSER_NOT_EMPTY_TOOLTIP,
        "Composer must be empty to edit a message"
    );
    assert_eq!(
        TranscriptEditDisabledReason::ComposerNotEmpty.tooltip(),
        EDIT_COMPOSER_NOT_EMPTY_TOOLTIP
    );
}

fn allowed_gate() -> TranscriptEditMenuGate {
    TranscriptEditMenuGate {
        transcript_selection_active: false,
        source_thread_idle: true,
        selected_thread_matches_target: true,
        selected_thread_compaction_active: false,
        pending_thread_activation: false,
        rollback_capability_available: true,
        composer_empty: true,
        pending_turn_input: false,
        pending_active_turn_steering: false,
        conflicting_selected_thread_work: false,
        image_label_readiness: ComposerImagePasteReadiness::Ready,
    }
}

fn assert_unavailable(target: shell::TranscriptEditTarget, gate: TranscriptEditMenuGate) {
    assert_eq!(
        transcript_edit_menu_entry(TranscriptEditTargetResolution::Enabled(target), gate),
        None
    );
}

fn disabled_reason_at(harness: &EditHarness, index: usize) -> Option<TranscriptEditDisabledReason> {
    match harness.target_resolution_at(index, true)? {
        TranscriptEditTargetResolution::Enabled(_) => None,
        TranscriptEditTargetResolution::Disabled { reason, .. } => Some(reason),
    }
}

fn prompt_turn_with_fragments(id: &str, prompts: &[&str]) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: prompts
                .iter()
                .map(|prompt| UserInput::Text {
                    text: (*prompt).to_string(),
                })
                .collect(),
        })],
        error: None,
    }
}

fn assistant_narrative_turn(id: &str, prompt: &str, answer: &str) -> TurnInfo {
    let mut turn = prompt_turn_with_fragments(id, &[prompt]);
    turn.items.push(ThreadItem::AgentMessage(AgentMessageItem {
        id: format!("{id}_answer"),
        phase: Some(ProtocolPhase::FinalAnswer),
        text: answer.to_string(),
    }));
    turn
}

fn assistant_only_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::AgentMessage(AgentMessageItem {
            id: format!("{id}_answer"),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: "Assistant text without a user prompt.".to_string(),
        })],
        error: None,
    }
}

fn operational_only_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::CommandExecution(CommandExecutionItem {
            id: format!("{id}_command"),
            command: "cargo metadata".to_string(),
            cwd: "C:\\repo".to_string(),
            status: CommandExecutionStatus::Completed,
            process_id: None,
            aggregated_output: Some("{}".to_string()),
            exit_code: Some(0),
            duration_ms: Some(10),
        })],
        error: None,
    }
}

fn local_image_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![UserInput::LocalImage {
                path: "C:\\image.png".to_string(),
            }],
        })],
        error: None,
    }
}

fn labeled_local_image_turn(id: &str, label: &str, path: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![
                UserInput::text(format!("Image {label}:")),
                UserInput::local_image(path.to_string()),
            ],
        })],
        error: None,
    }
}

fn mixed_local_image_turn(id: &str, label: &str, path: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![
                UserInput::text("before ".to_string()),
                UserInput::text(format!("Image {label}:")),
                UserInput::local_image(path.to_string()),
                UserInput::text(" after".to_string()),
            ],
        })],
        error: None,
    }
}

fn multi_fragment_text_and_image_turn(id: &str, label: &str, path: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![
            ThreadItem::UserMessage(UserMessageItem {
                id: format!("{id}_user_1"),
                content: vec![UserInput::text("first".to_string())],
            }),
            ThreadItem::UserMessage(UserMessageItem {
                id: format!("{id}_user_2"),
                content: vec![
                    UserInput::text("look ".to_string()),
                    UserInput::text(format!("Image {label}:")),
                    UserInput::local_image(path.to_string()),
                ],
            }),
            ThreadItem::UserMessage(UserMessageItem {
                id: format!("{id}_user_3"),
                content: vec![UserInput::text("last".to_string())],
            }),
        ],
        error: None,
    }
}

fn mention_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![UserInput::Mention {
                name: "README".to_string(),
                path: "README.md".to_string(),
            }],
        })],
        error: None,
    }
}
