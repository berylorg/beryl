#![allow(dead_code, private_interfaces, unused_imports)]

use std::{path::PathBuf, sync::Arc};

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
        TranscriptBranchTarget, TranscriptImageMenuTarget, transcript_branch_menu_can_open,
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
                        items: Vec::new(),
                        error: None,
                    },
                })
                .expect("live turn should accept turn start");
            let turn = self.details.turns()[index].clone();
            self.presentation.replace_turn(index, turn);
        }

        pub(super) fn target_at(&self, index: usize) -> Option<TranscriptBranchTarget> {
            self.presentation
                .turn_at(index)
                .and_then(|row| TranscriptBranchTarget::from_presented_row(&row))
        }
    }
}

use shell::{
    BranchHarness, TranscriptBranchAction, TranscriptBranchMenuOpenGate, TranscriptBranchMenuState,
    TranscriptEditMenuEntry, TranscriptEditTarget, TranscriptImageMenuTarget,
    transcript_branch_menu_can_open,
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
fn branch_target_rejects_blank_prompt_rows() {
    let mut harness = BranchHarness::new();
    harness.replace_history(
        "thread_a",
        vec![prompt_turn_with_fragments("turn_1", &["   ", "\n\t"])],
    );

    assert!(harness.target_at(0).is_none());
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
    assert!(copied.clipboard_item().entries().iter().any(
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
