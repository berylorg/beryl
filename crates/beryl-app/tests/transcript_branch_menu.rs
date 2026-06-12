#![allow(dead_code, private_interfaces, unused_imports)]

use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc};

use beryl_backend::{ThreadItem, TurnInfo, TurnStatus, UserInput, UserMessageItem};
use gpui::{Bounds, ClipboardEntry, Image, ImageFormat, point, px, size};

mod shell {
    #[path = "../../src/shell/composer_draft.rs"]
    mod composer_draft;
    #[path = "../../src/shell/composer_image_labels.rs"]
    mod composer_image_labels;
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_branch_menu_state.rs"]
    pub(super) mod transcript_branch_menu_state;
    #[path = "../../src/shell/transcript_edit_menu_state.rs"]
    mod transcript_edit_menu_state;
    #[path = "../../src/shell/transcript_image_menu_actions.rs"]
    pub(super) mod transcript_image_menu_actions;
    #[path = "../../src/shell/transcript_presentation.rs"]
    mod transcript_presentation;
    #[path = "../../src/shell/transcript_projection.rs"]
    mod transcript_projection;
    #[allow(dead_code)]
    #[path = "../../src/shell/virtual_list/mod.rs"]
    mod virtual_list;

    use beryl_backend::{TurnInfo, TurnStreamEvent, UserInput};

    pub(super) use self::transcript_branch_menu_state::{
        TranscriptBranchAction, TranscriptBranchMenuOpenGate, TranscriptBranchMenuState,
        TranscriptBranchTarget, TranscriptImageMenuTarget,
        TranscriptThreadTitleUpdateDisabledReason, TranscriptThreadTitleUpdateMenuEntry,
        TranscriptThreadTitleUpdateMenuGate, TranscriptThreadTitleUpdateTarget,
        TranscriptThreadTitleUpdateTargetResolution, transcript_branch_menu_can_open,
        transcript_thread_title_update_menu_entry,
    };
    pub(super) use self::transcript_edit_menu_state::{
        TranscriptEditMenuEntry, TranscriptEditTarget,
    };
    pub(super) use self::transcript_image_menu_actions::{
        copy_transcript_image_to_clipboard, save_transcript_image_as,
    };
    use self::{
        execution_detail::{
            TranscriptImageMarkerSpec, TranscriptImagePreviewState, UserInputFragment,
            transcript_image_source_from_local_image,
        },
        transcript_presentation::TranscriptPresentationState,
    };

    pub(super) struct BranchHarness {
        details: execution_detail::ExecutionDetailState,
        presentation: TranscriptPresentationState,
    }

    impl BranchHarness {
        pub(super) fn new() -> Self {
            Self {
                details: execution_detail::ExecutionDetailState::default(),
                presentation: TranscriptPresentationState::default(),
            }
        }

        pub(super) fn replace_history(&mut self, thread_id: &str, turns: Vec<TurnInfo>) {
            self.details = execution_detail::ExecutionDetailState::default();
            self.details.prepend_thread_history_page(thread_id, turns);
            self.presentation.replace_from_turns(self.details.turns());
        }

        pub(super) fn release_range(&mut self, range: std::ops::Range<usize>) -> usize {
            let replacements = self.details.release_history_range(range);
            let count = replacements.len();
            for replacement in replacements {
                self.presentation
                    .replace_turn(replacement.index, replacement.turn);
            }
            count
        }

        pub(super) fn begin_live_image_turn(
            &mut self,
            text: &str,
            marker_range: std::ops::Range<usize>,
        ) {
            let fragment = UserInputFragment::from_backend_input_with_image_markers(
                text.to_string(),
                vec![
                    UserInput::Text {
                        text: text.to_string(),
                    },
                    UserInput::LocalImage {
                        path: "C:\\image.png".to_string(),
                    },
                ],
                vec![TranscriptImageMarkerSpec::new(
                    "A",
                    marker_range,
                    transcript_image_source_from_local_image(
                        "C:\\image.png",
                        Some("asset_a".to_string()),
                        TranscriptImagePreviewState::Available,
                    ),
                )],
            );
            let turn_index = self.details.begin_turn_with_fragments(vec![fragment]);
            let turn = self.details.turns()[turn_index].clone();
            self.presentation
                .append_turn(turn_index, turn)
                .expect("image prompt should project into transcript");
        }

        pub(super) fn materialize_live_turn(&mut self, thread_id: &str, turn_id: &str) {
            let index = self
                .details
                .apply_stream_event(TurnStreamEvent::TurnStarted {
                    thread_id: thread_id.to_string(),
                    turn: TurnInfo {
                        id: turn_id.to_string(),
                        status: beryl_backend::TurnStatus::InProgress,
                        items_view: beryl_backend::TurnItemsView::Full,
                        items: Vec::new(),
                        error: None,
                    },
                })
                .expect("live turn should accept turn start");
            let turn = self.details.turns()[index].clone();
            self.presentation.replace_turn(index, turn).row_index();
        }

        pub(super) fn target_at(&self, index: usize) -> Option<TranscriptBranchTarget> {
            self.presentation
                .turn_at(index)
                .and_then(|row| TranscriptBranchTarget::from_presented_row(&row))
        }

        pub(super) fn title_target_at(
            &self,
            index: usize,
        ) -> Option<TranscriptThreadTitleUpdateTarget> {
            self.presentation
                .turn_at(index)
                .and_then(|row| TranscriptThreadTitleUpdateTarget::from_presented_row(&row))
        }

        pub(super) fn title_target_resolution_at(
            &self,
            index: usize,
        ) -> Option<TranscriptThreadTitleUpdateTargetResolution> {
            self.presentation
                .turn_at(index)
                .and_then(|row| TranscriptThreadTitleUpdateTarget::resolve_from_presented_row(&row))
        }

        pub(super) fn presentation_len(&self) -> usize {
            self.presentation.len()
        }
    }
}

use shell::{
    BranchHarness, TranscriptBranchAction, TranscriptBranchMenuOpenGate, TranscriptBranchMenuState,
    TranscriptEditMenuEntry, TranscriptEditTarget, TranscriptImageMenuTarget,
    TranscriptThreadTitleUpdateDisabledReason, TranscriptThreadTitleUpdateMenuEntry,
    TranscriptThreadTitleUpdateMenuGate, TranscriptThreadTitleUpdateTarget,
    TranscriptThreadTitleUpdateTargetResolution, transcript_branch_menu_can_open,
    transcript_thread_title_update_menu_entry,
};

#[test]
fn branch_target_extracts_exact_thread_turn_index_and_ordered_title_seed() {
    let mut harness = BranchHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn_with_fragments("turn_1", &["First fragment", "Second fragment"]),
            prompt_turn_with_fragments("turn_2", &["Later fragment"]),
        ],
    );

    let target = harness
        .target_at(0)
        .expect("first row should be branchable");

    assert_eq!(target.source_thread_id(), "thread_a");
    assert_eq!(target.source_turn_id(), "turn_1");
    assert_eq!(target.source_turn_index(), 0);
    assert_eq!(
        target.title_seed_fragments(),
        &["First fragment".to_string(), "Second fragment".to_string()]
    );
    assert_eq!(
        target.title_seed_text(),
        "First fragment\n\nSecond fragment"
    );
}

#[test]
fn branch_target_replaces_transcript_image_markers_with_copy_fallback_text() {
    let mut harness = BranchHarness::new();
    harness.begin_live_image_turn("Look at [A]", 8..11);
    harness.materialize_live_turn("thread_a", "turn_1");

    let target = harness
        .target_at(0)
        .expect("image row should be branchable");

    assert_eq!(
        target.title_seed_fragments(),
        &["Look at [Image A]".to_string()]
    );
}

#[test]
fn oversized_fallback_row_is_not_a_branch_or_title_update_target() {
    let mut harness = BranchHarness::new();
    harness.replace_history("thread_a", vec![oversized_fallback_turn("turn_big")]);

    assert_eq!(harness.presentation_len(), 1);
    assert!(harness.target_at(0).is_none());
    assert!(harness.title_target_at(0).is_none());
    assert!(matches!(
        harness.title_target_resolution_at(0),
        Some(TranscriptThreadTitleUpdateTargetResolution::Disabled {
            reason: TranscriptThreadTitleUpdateDisabledReason::NonResidentRowTarget,
            ..
        })
    ));
}

#[test]
fn thread_title_update_target_extracts_clicked_turn_seed() {
    let mut harness = BranchHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn_with_fragments("turn_1", &["First fragment", "Second fragment"]),
            prompt_turn_with_fragments("turn_2", &["Later fragment"]),
        ],
    );

    let target = harness
        .title_target_at(0)
        .expect("first row should support title updates");

    assert_eq!(target.source_thread_id(), "thread_a");
    assert_eq!(target.source_turn_id(), "turn_1");
    assert_eq!(target.source_turn_index(), 0);
    assert_eq!(
        target.title_seed_fragments(),
        &["First fragment".to_string(), "Second fragment".to_string()]
    );
    assert_eq!(
        target.title_seed_text(),
        "First fragment\n\nSecond fragment"
    );
}

#[test]
fn thread_title_update_target_replaces_image_markers_with_copy_fallback_text() {
    let mut harness = BranchHarness::new();
    harness.begin_live_image_turn("Look at [A]", 8..11);
    harness.materialize_live_turn("thread_a", "turn_1");

    let target = harness
        .title_target_at(0)
        .expect("image row should support title updates");

    assert_eq!(
        target.title_seed_fragments(),
        &["Look at [Image A]".to_string()]
    );
}

#[test]
fn branch_target_rejects_blank_prompt_rows() {
    let mut harness = BranchHarness::new();
    harness.replace_history(
        "thread_a",
        vec![prompt_turn_with_fragments("turn_1", &["   ", "\n\t"])],
    );

    assert!(harness.target_at(0).is_none());
}

#[test]
fn thread_title_update_target_reports_missing_seed_as_disabled_reason() {
    let mut harness = BranchHarness::new();
    harness.replace_history(
        "thread_a",
        vec![prompt_turn_with_fragments("turn_1", &["   ", "\n\t"])],
    );

    let resolution = harness
        .title_target_resolution_at(0)
        .expect("blank prompt row should still produce a disabled title-update entry");

    assert_eq!(
        resolution,
        TranscriptThreadTitleUpdateTargetResolution::Disabled {
            identity: Some(
                TranscriptThreadTitleUpdateTarget::for_test("thread_a", "turn_1", 0, Vec::new())
                    .target_identity()
                    .clone()
            ),
            reason: TranscriptThreadTitleUpdateDisabledReason::MissingUserInputSeed,
        }
    );
}

#[test]
fn thread_title_update_target_reports_released_row_as_disabled_reason() {
    let mut harness = BranchHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn_with_fragments("turn_1", &["Prompt 1"]),
            prompt_turn_with_fragments("turn_2", &["Prompt 2"]),
        ],
    );

    assert_eq!(harness.release_range(0..1), 1);
    assert_eq!(harness.presentation_len(), 1);

    let target = harness
        .title_target_at(0)
        .expect("remaining row should still support title updates");
    assert_eq!(target.source_thread_id(), "thread_a");
    assert_eq!(target.source_turn_id(), "turn_2");
    assert_eq!(target.source_turn_index(), 1);
    assert_eq!(target.title_seed_fragments(), &["Prompt 2".to_string()]);
}

#[test]
fn branch_menu_open_gate_requires_selection_free_idle_exact_supported_context() {
    let allowed = TranscriptBranchMenuOpenGate {
        transcript_selection_active: false,
        source_thread_idle: true,
        selected_thread_matches_target: true,
        selected_thread_compaction_active: false,
        pending_thread_activation: false,
        branch_capability_available: true,
    };
    assert!(transcript_branch_menu_can_open(allowed));

    assert!(!transcript_branch_menu_can_open(
        TranscriptBranchMenuOpenGate {
            transcript_selection_active: true,
            ..allowed
        }
    ));
    assert!(!transcript_branch_menu_can_open(
        TranscriptBranchMenuOpenGate {
            source_thread_idle: false,
            ..allowed
        }
    ));
    assert!(!transcript_branch_menu_can_open(
        TranscriptBranchMenuOpenGate {
            selected_thread_matches_target: false,
            ..allowed
        }
    ));
    assert!(!transcript_branch_menu_can_open(
        TranscriptBranchMenuOpenGate {
            selected_thread_compaction_active: true,
            ..allowed
        }
    ));
    assert!(!transcript_branch_menu_can_open(
        TranscriptBranchMenuOpenGate {
            pending_thread_activation: true,
            ..allowed
        }
    ));
    assert!(!transcript_branch_menu_can_open(
        TranscriptBranchMenuOpenGate {
            branch_capability_available: false,
            ..allowed
        }
    ));
}

#[test]
fn thread_title_update_menu_entry_allows_explicit_request_for_registered_thread_with_backend_title()
{
    let target = TranscriptThreadTitleUpdateTarget::for_test(
        "external_thread",
        "turn_1",
        0,
        vec!["Already titled prompt".to_string()],
    );

    let entry = transcript_thread_title_update_menu_entry(
        TranscriptThreadTitleUpdateTargetResolution::Enabled(target.clone()),
        title_update_gate(),
    )
    .expect("title-update entry should be produced");

    assert_eq!(entry.disabled_reason(), None);
    let request = entry
        .into_request()
        .expect("enabled title-update entry should produce a request");
    assert_eq!(request.target(), &target);
}

#[test]
fn thread_title_update_menu_entry_disables_manual_title_and_duplicate_worker_cases() {
    let target = TranscriptThreadTitleUpdateTarget::for_test(
        "thread_a",
        "turn_1",
        0,
        vec!["Prompt".to_string()],
    );

    let manual_title_entry = transcript_thread_title_update_menu_entry(
        TranscriptThreadTitleUpdateTargetResolution::Enabled(target.clone()),
        TranscriptThreadTitleUpdateMenuGate {
            manual_title_visible: true,
            ..title_update_gate()
        },
    )
    .expect("manual-title case should still be visible as disabled");
    assert_eq!(
        manual_title_entry.disabled_reason(),
        Some(TranscriptThreadTitleUpdateDisabledReason::ManualTitleVisible)
    );

    let duplicate_worker_entry = transcript_thread_title_update_menu_entry(
        TranscriptThreadTitleUpdateTargetResolution::Enabled(target),
        TranscriptThreadTitleUpdateMenuGate {
            title_task_active: true,
            ..title_update_gate()
        },
    )
    .expect("duplicate-worker case should still be visible as disabled");
    assert_eq!(
        duplicate_worker_entry.disabled_reason(),
        Some(TranscriptThreadTitleUpdateDisabledReason::TitleWorkerAlreadyActive)
    );
}

#[test]
fn thread_title_update_menu_entry_represents_dispatch_prerequisites_as_disabled_reasons() {
    let target = TranscriptThreadTitleUpdateTarget::for_test(
        "thread_a",
        "turn_1",
        0,
        vec!["Prompt".to_string()],
    );

    let missing_backend_entry = transcript_thread_title_update_menu_entry(
        TranscriptThreadTitleUpdateTargetResolution::Enabled(target.clone()),
        TranscriptThreadTitleUpdateMenuGate {
            backend_connector_available: false,
            ..title_update_gate()
        },
    )
    .expect("missing-backend case should still be visible as disabled");
    assert_eq!(
        missing_backend_entry.disabled_reason(),
        Some(TranscriptThreadTitleUpdateDisabledReason::BackendConnectorUnavailable)
    );

    let capacity_entry = transcript_thread_title_update_menu_entry(
        TranscriptThreadTitleUpdateTargetResolution::Enabled(target),
        TranscriptThreadTitleUpdateMenuGate {
            title_worker_capacity_available: false,
            ..title_update_gate()
        },
    )
    .expect("capacity case should still be visible as disabled");
    assert_eq!(
        capacity_entry.disabled_reason(),
        Some(TranscriptThreadTitleUpdateDisabledReason::TitleWorkerCapacityReached)
    );
}

#[test]
fn branch_menu_tracks_bounds_dismissal_and_action_acceptance() {
    let mut harness = BranchHarness::new();
    harness.replace_history(
        "thread_a",
        vec![prompt_turn_with_fragments("turn_1", &["Prompt"])],
    );
    let target = harness.target_at(0).expect("target should be branchable");
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_target(target.clone(), point(px(120.0), px(80.0)));
    assert!(menu.is_open());
    assert!(menu.should_dismiss_for_mouse_down(point(px(120.0), px(90.0))));

    menu.set_bounds(Some(Bounds::new(
        point(px(100.0), px(70.0)),
        size(px(200.0), px(90.0)),
    )));
    assert!(!menu.should_dismiss_for_mouse_down(point(px(120.0), px(90.0))));
    assert!(menu.should_dismiss_for_mouse_down(point(px(40.0), px(90.0))));

    let request = menu
        .accept(TranscriptBranchAction::SwitchTo)
        .expect("open menu should accept an action");
    assert_eq!(request.action(), TranscriptBranchAction::SwitchTo);
    assert_eq!(request.target(), &target);
    assert!(!menu.is_open());

    menu.open_target(target, point(px(120.0), px(80.0)));
    let request = menu
        .accept(TranscriptBranchAction::Background)
        .expect("reopened menu should accept background action");
    assert_eq!(request.action(), TranscriptBranchAction::Background);
}

#[test]
fn shared_turn_menu_accepts_edit_entry_without_changing_branch_actions() {
    let branch_target = shell::TranscriptBranchTarget::for_test(
        "thread_a",
        "turn_2",
        1,
        vec!["Prompt".to_string()],
    );
    let edit_target =
        TranscriptEditTarget::for_test("thread_a", "turn_2", 1, 2, vec!["Prompt".to_string()]);
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_menu(
        Some(branch_target.clone()),
        Some(TranscriptEditMenuEntry::Enabled(edit_target.clone())),
        None,
        point(px(120.0), px(80.0)),
    );
    let edit_request = menu
        .accept_edit()
        .expect("enabled edit row should produce edit request");
    assert_eq!(edit_request.target(), &edit_target);
    assert!(!menu.is_open());

    menu.open_menu(
        Some(branch_target.clone()),
        Some(TranscriptEditMenuEntry::Enabled(edit_target)),
        None,
        point(px(120.0), px(80.0)),
    );
    let branch_request = menu
        .accept(TranscriptBranchAction::SwitchTo)
        .expect("branch row should still produce branch request");
    assert_eq!(branch_request.action(), TranscriptBranchAction::SwitchTo);
    assert_eq!(branch_request.target(), &branch_target);
    assert!(!menu.is_open());
}

#[test]
fn shared_turn_menu_accepts_thread_title_update_entry_without_changing_other_actions() {
    let branch_target = shell::TranscriptBranchTarget::for_test(
        "thread_a",
        "turn_2",
        1,
        vec!["Prompt".to_string()],
    );
    let title_target = TranscriptThreadTitleUpdateTarget::for_test(
        "thread_a",
        "turn_2",
        1,
        vec!["Prompt".to_string()],
    );
    let edit_target =
        TranscriptEditTarget::for_test("thread_a", "turn_2", 1, 2, vec!["Prompt".to_string()]);
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_menu_with_title_update(
        Some(branch_target.clone()),
        Some(TranscriptEditMenuEntry::Enabled(edit_target.clone())),
        Some(TranscriptThreadTitleUpdateMenuEntry::Enabled(
            title_target.clone(),
        )),
        None,
        point(px(120.0), px(80.0)),
    );
    let title_request = menu
        .accept_thread_title_update()
        .expect("enabled title-update row should produce a request");
    assert_eq!(title_request.target(), &title_target);
    assert!(!menu.is_open());

    menu.open_menu_with_title_update(
        Some(branch_target.clone()),
        Some(TranscriptEditMenuEntry::Enabled(edit_target)),
        Some(TranscriptThreadTitleUpdateMenuEntry::Enabled(title_target)),
        None,
        point(px(120.0), px(80.0)),
    );
    let branch_request = menu
        .accept(TranscriptBranchAction::SwitchTo)
        .expect("branch row should still produce branch request");
    assert_eq!(branch_request.action(), TranscriptBranchAction::SwitchTo);
    assert_eq!(branch_request.target(), &branch_target);
    assert!(!menu.is_open());
}

#[test]
fn title_update_only_menu_opens_and_accepts_enabled_entry() {
    let title_target = TranscriptThreadTitleUpdateTarget::for_test(
        "thread_a",
        "turn_2",
        1,
        vec!["Prompt".to_string()],
    );
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_menu_with_title_update(
        None,
        None,
        Some(TranscriptThreadTitleUpdateMenuEntry::Enabled(
            title_target.clone(),
        )),
        None,
        point(px(120.0), px(80.0)),
    );

    let active = menu
        .active()
        .expect("title-update-only menu should be open");
    assert!(active.branch_target().is_none());
    assert!(active.edit_entry().is_none());
    assert!(active.image_target().is_none());
    assert!(active.title_update_entry().is_some());

    let title_request = menu
        .accept_thread_title_update()
        .expect("enabled title-update-only row should produce a request");
    assert_eq!(title_request.target(), &title_target);
    assert!(!menu.is_open());
}

#[test]
fn disabled_thread_title_update_entry_stays_visible_and_rejects_acceptance() {
    let title_target = TranscriptThreadTitleUpdateTarget::for_test(
        "thread_a",
        "turn_2",
        1,
        vec!["Prompt".to_string()],
    );
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_menu_with_title_update(
        None,
        None,
        Some(TranscriptThreadTitleUpdateMenuEntry::Disabled {
            identity: Some(title_target.target_identity().clone()),
            reason: TranscriptThreadTitleUpdateDisabledReason::ManualTitleVisible,
        }),
        None,
        point(px(120.0), px(80.0)),
    );

    let entry = menu
        .active()
        .and_then(|open| open.title_update_entry())
        .expect("disabled title-update row should remain visible");
    assert_eq!(
        entry.disabled_reason(),
        Some(TranscriptThreadTitleUpdateDisabledReason::ManualTitleVisible)
    );
    assert!(menu.accept_thread_title_update().is_none());
    assert!(!menu.is_open());
}

#[test]
fn shared_turn_menu_accepts_image_target_without_changing_turn_actions() {
    let branch_target = shell::TranscriptBranchTarget::for_test(
        "thread_a",
        "turn_2",
        1,
        vec!["Prompt".to_string()],
    );
    let edit_target =
        TranscriptEditTarget::for_test("thread_a", "turn_2", 1, 2, vec!["Prompt".to_string()]);
    let image_target = image_menu_target();
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_menu(
        Some(branch_target.clone()),
        Some(TranscriptEditMenuEntry::Enabled(edit_target.clone())),
        Some(image_target.clone()),
        point(px(120.0), px(80.0)),
    );
    let active_image = menu
        .active()
        .and_then(|open| open.image_target())
        .expect("image target should be available while menu is open");
    assert_eq!(active_image.row_identity(), "thread:thread_a:turn:turn_2");
    assert_eq!(active_image.media_identity(), "media:rendered-image");
    assert_eq!(active_image.alt(), "Rendered image");
    assert_eq!(active_image.format(), ImageFormat::Png);
    assert_eq!(active_image.bytes(), b"image bytes");
    assert_eq!(active_image.source_path(), Some("C:\\image.png"));

    let copied = menu
        .accept_copy_image()
        .expect("copy image should consume the open image target");
    assert_eq!(copied.row_identity(), image_target.row_identity());
    let clipboard_item = copied
        .clipboard_item()
        .expect("retained image clipboard item should be created");
    assert!(clipboard_item.entries().iter().any(
        |entry| matches!(entry, ClipboardEntry::Image(image) if image.bytes == b"image bytes")
    ));
    assert!(!menu.is_open());

    menu.open_menu(
        Some(branch_target.clone()),
        Some(TranscriptEditMenuEntry::Enabled(edit_target)),
        Some(image_target),
        point(px(120.0), px(80.0)),
    );
    let branch_request = menu
        .accept(TranscriptBranchAction::Background)
        .expect("branch row should still produce branch request");
    assert_eq!(branch_request.action(), TranscriptBranchAction::Background);
    assert_eq!(branch_request.target(), &branch_target);
    assert!(!menu.is_open());
}

#[test]
fn image_target_matches_exact_rendered_media_identity() {
    let target = image_menu_target();

    assert!(target.matches_rendered_media("thread:thread_a:turn:turn_2", "media:rendered-image"));
    assert!(!target.matches_rendered_media("thread:thread_a:turn:turn_2", "media:other-image"));
    assert!(!target.matches_rendered_media("thread:thread_a:turn:turn_3", "media:rendered-image"));
}

#[test]
fn image_target_matches_exact_loaded_image_bytes() {
    let target = image_menu_target_with_bytes(
        "Rendered image",
        ImageFormat::Png,
        b"image bytes",
        Some("C:\\image.png".to_string()),
    );
    let same_loaded_image = image_menu_target_with_bytes(
        "Rendered image",
        ImageFormat::Png,
        b"image bytes",
        Some("C:\\image.png".to_string()),
    );
    let reloaded_bytes = image_menu_target_with_bytes(
        "Rendered image",
        ImageFormat::Png,
        b"updated image bytes",
        Some("C:\\image.png".to_string()),
    );

    assert!(target.matches_loaded_image(&same_loaded_image));
    assert!(!target.matches_loaded_image(&reloaded_bytes));
}

#[test]
fn image_target_keeps_shared_image_bytes() {
    let bytes: Arc<[u8]> = Arc::from(&b"image bytes"[..]);
    let bytes_ptr = bytes.as_ptr();

    let target = TranscriptImageMenuTarget::new(
        "thread:thread_a:turn:turn_2",
        "media:rendered-image",
        "Rendered image",
        ImageFormat::Png,
        bytes.clone(),
        Arc::new(Image::from_bytes(ImageFormat::Png, b"image bytes".to_vec())),
        None,
    );

    assert_eq!(target.bytes(), b"image bytes");
    assert_eq!(target.bytes_arc().as_ptr(), bytes_ptr);
    assert_eq!(target.bytes_ptr(), bytes_ptr);
}

#[test]
fn image_target_file_source_reads_clipboard_bytes_on_demand() {
    let temp = tempfile::tempdir().expect("temp dir should be created");
    let source_path = temp.path().join("rendered.png");
    fs::write(&source_path, b"initial image bytes").expect("source image should be written");
    let target = TranscriptImageMenuTarget::new_file(
        "thread:thread_a:turn:turn_2",
        "media:rendered-image",
        "Rendered image",
        ImageFormat::Png,
        source_path.clone(),
        Some(source_path.to_string_lossy().to_string()),
    );
    fs::write(&source_path, b"updated image bytes").expect("source image should be updated");

    let clipboard_item = target
        .clipboard_item()
        .expect("file-backed clipboard item should be created");

    assert!(clipboard_item.entries().iter().any(
        |entry| matches!(entry, ClipboardEntry::Image(image) if image.bytes == b"updated image bytes")
    ));
    assert_eq!(target.retained_bytes(), None);
    assert_eq!(target.file_path(), Some(source_path.as_path()));
}

#[test]
fn image_target_file_source_save_copies_authoritative_file_on_demand() {
    let temp = tempfile::tempdir().expect("temp dir should be created");
    let source_path = temp.path().join("rendered.png");
    let output_path = temp.path().join("copy.png");
    fs::write(&source_path, b"initial image bytes").expect("source image should be written");
    let target = TranscriptImageMenuTarget::new_file(
        "thread:thread_a:turn:turn_2",
        "media:rendered-image",
        "Rendered image",
        ImageFormat::Png,
        source_path.clone(),
        Some(source_path.to_string_lossy().to_string()),
    );
    fs::write(&source_path, b"updated image bytes").expect("source image should be updated");

    let saved_path = target
        .save_to_path(output_path.clone())
        .expect("file-backed image should save");

    assert_eq!(saved_path, output_path);
    assert_eq!(fs::read(output_path).unwrap(), b"updated image bytes");
}

#[test]
fn clearing_stale_image_target_preserves_turn_actions() {
    let branch_target = shell::TranscriptBranchTarget::for_test(
        "thread_a",
        "turn_2",
        1,
        vec!["Prompt".to_string()],
    );
    let edit_target =
        TranscriptEditTarget::for_test("thread_a", "turn_2", 1, 2, vec!["Prompt".to_string()]);
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_menu(
        Some(branch_target.clone()),
        Some(TranscriptEditMenuEntry::Enabled(edit_target.clone())),
        Some(image_menu_target()),
        point(px(120.0), px(80.0)),
    );

    assert!(menu.clear_image_target());
    let active = menu
        .active()
        .expect("turn menu should stay open after stale image target clears");
    assert!(active.image_target().is_none());
    assert!(active.branch_target().is_some());
    assert!(active.edit_entry().is_some());

    let branch_request = menu
        .accept(TranscriptBranchAction::SwitchTo)
        .expect("remaining branch action should still work");
    assert_eq!(branch_request.target(), &branch_target);
}

#[test]
fn clearing_stale_image_target_preserves_title_update_entry() {
    let title_target = TranscriptThreadTitleUpdateTarget::for_test(
        "thread_a",
        "turn_2",
        1,
        vec!["Prompt".to_string()],
    );
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_menu_with_title_update(
        None,
        None,
        Some(TranscriptThreadTitleUpdateMenuEntry::Enabled(
            title_target.clone(),
        )),
        Some(image_menu_target()),
        point(px(120.0), px(80.0)),
    );

    assert!(menu.clear_image_target());
    let active = menu
        .active()
        .expect("title-update menu should stay open after stale image target clears");
    assert!(active.image_target().is_none());
    assert!(active.title_update_entry().is_some());

    let title_request = menu
        .accept_thread_title_update()
        .expect("remaining title-update action should still work");
    assert_eq!(title_request.target(), &title_target);
}

#[test]
fn clearing_stale_image_only_target_closes_menu() {
    let mut menu = TranscriptBranchMenuState::default();

    menu.open_menu(
        None,
        None,
        Some(image_menu_target()),
        point(px(120.0), px(80.0)),
    );

    assert!(menu.clear_image_target());
    assert!(!menu.is_open());
}

#[test]
fn shared_turn_menu_accepts_save_image_target() {
    let mut menu = TranscriptBranchMenuState::default();
    let image_target = image_menu_target();

    menu.open_menu(
        None,
        None,
        Some(image_target.clone()),
        point(px(120.0), px(80.0)),
    );

    let saved = menu
        .accept_save_image()
        .expect("save image should consume the open image target");
    assert_eq!(saved.row_identity(), image_target.row_identity());
    assert_eq!(saved.bytes(), b"image bytes");
    assert!(!menu.is_open());
}

#[test]
fn image_only_turn_menu_opens_for_loaded_media_target() {
    let mut menu = TranscriptBranchMenuState::default();
    let image_target = image_menu_target();

    menu.open_menu(None, None, Some(image_target), point(px(120.0), px(80.0)));

    assert!(menu.is_open());
    assert!(menu.active().and_then(|open| open.image_target()).is_some());
}

#[test]
fn image_save_filename_prefers_source_file_name_with_extension() {
    let target = image_menu_target_with(
        "Rendered image",
        ImageFormat::Png,
        Some("C:\\runs\\output\\rendered.final.png".to_string()),
    );

    assert_eq!(target.suggested_save_filename(), "rendered.final.png");
    assert_eq!(target.save_extension(), "png");
}

#[test]
fn image_save_filename_uses_sanitized_alt_and_format_extension() {
    let target = image_menu_target_with(" Rendered: image? * ", ImageFormat::Jpeg, None);

    assert_eq!(target.suggested_save_filename(), "Rendered image.jpg");
    assert_eq!(target.save_extension(), "jpg");
}

#[test]
fn image_save_filename_uses_default_when_metadata_is_blank() {
    let target = image_menu_target_with("   ", ImageFormat::Webp, None);

    assert_eq!(target.suggested_save_filename(), "transcript-image.webp");
    assert_eq!(target.save_extension(), "webp");
}

#[test]
fn image_save_path_adds_format_extension_only_when_missing() {
    let target = image_menu_target_with("Rendered image", ImageFormat::Jpeg, None);

    assert_eq!(
        target.save_path_with_default_extension(PathBuf::from("C:\\out\\chosen")),
        PathBuf::from("C:\\out\\chosen.jpg")
    );
    assert_eq!(
        target.save_path_with_default_extension(PathBuf::from("C:\\out\\chosen.custom")),
        PathBuf::from("C:\\out\\chosen.custom")
    );
}

fn prompt_turn_with_fragments(id: &str, prompts: &[&str]) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
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

fn oversized_fallback_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Summary,
        items: vec![ThreadItem::Generic(beryl_backend::GenericThreadItem {
            id: format!("beryl:oversized-turn-fallback:{id}"),
            item_type: "beryl.oversizedTurnFallback".to_string(),
            tool: None,
            server: None,
            namespace: None,
            mcp_app_resource_uri: None,
            status: None,
            model: None,
            reasoning_effort: None,
            receiver_thread_ids: Vec::new(),
            agents_states: BTreeMap::new(),
            agent_nickname: None,
        })],
        error: None,
    }
}

fn image_menu_target() -> TranscriptImageMenuTarget {
    image_menu_target_with(
        "Rendered image",
        ImageFormat::Png,
        Some("C:\\image.png".to_string()),
    )
}

fn image_menu_target_with(
    alt: &str,
    format: ImageFormat,
    source_path: Option<String>,
) -> TranscriptImageMenuTarget {
    image_menu_target_with_bytes(alt, format, b"image bytes", source_path)
}

fn image_menu_target_with_bytes(
    alt: &str,
    format: ImageFormat,
    bytes: &[u8],
    source_path: Option<String>,
) -> TranscriptImageMenuTarget {
    TranscriptImageMenuTarget::new(
        "thread:thread_a:turn:turn_2",
        "media:rendered-image",
        alt,
        format,
        bytes.to_vec(),
        Arc::new(Image::from_bytes(format, bytes.to_vec())),
        source_path,
    )
}

fn title_update_gate() -> TranscriptThreadTitleUpdateMenuGate {
    TranscriptThreadTitleUpdateMenuGate {
        transcript_selection_active: false,
        selected_thread_matches_target: true,
        selected_thread_compaction_active: false,
        pending_thread_activation: false,
        manual_title_visible: false,
        title_task_active: false,
        backend_connector_available: true,
        title_worker_capacity_available: true,
    }
}
