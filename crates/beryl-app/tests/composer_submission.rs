#![allow(dead_code)]

#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{
    BerylWorkspacePersistence, WorkspaceImageAsset, WorkspaceImageAssetStatus,
    WorkspacePersistenceError,
};
use beryl_backend::UserInput;
use beryl_model::workspace::{BerylWorkspaceId, RuntimeMode};
use gpui::ImageFormat;

#[path = "../src/shell/composer_draft.rs"]
mod composer_draft;
#[path = "../src/shell/composer_image_delivery.rs"]
mod composer_image_delivery;
#[path = "../src/shell/composer_submission.rs"]
mod composer_submission;
#[path = "../src/shell/execution_detail.rs"]
mod execution_detail;

use composer_draft::{ComposerDraft, ComposerDraftImageData};
use composer_image_delivery::{PreparedComposerDraft, prepare_accepted_composer_images};
use composer_submission::prepared_composer_draft_fragment;
use execution_detail::TranscriptImagePreviewState;

#[test]
fn prepared_composer_draft_serializes_text_image_order_with_generated_label_text() {
    let (root, persistence, workspace_id, image_data) = durable_png_data(b"image");
    let asset_id = image_data
        .asset_id()
        .expect("durable image data should carry asset id")
        .to_string();
    let mut draft = ComposerDraft::default();
    draft.sync_display_text("Look  now".to_string());
    draft.replace_range_with_image("Look ".len().."Look ".len(), "A", image_data);

    let accepted = draft.accepted().expect("image draft should be accepted");
    let prepared_images = prepare_accepted_composer_images(
        &persistence,
        &workspace_id,
        &accepted,
        &RuntimeMode::HostWindows,
    )
    .expect("image should prepare for host runtime");
    let prepared = PreparedComposerDraft::new(accepted, prepared_images);
    let backend_path = prepared
        .backend_path_for_label("A")
        .expect("prepared image should have path")
        .to_string();

    let fragment =
        prepared_composer_draft_fragment(&prepared).expect("prepared draft should serialize");

    assert_eq!(fragment.text, "Look [A] now");
    let markers = fragment.image_markers();
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].label(), "A");
    assert_eq!(markers[0].display_range(), 5..8);
    assert_eq!(markers[0].copy_text(), "[Image A]");
    assert_eq!(markers[0].source().asset_id(), Some(asset_id.as_str()));
    assert_eq!(
        markers[0].source().preview_state(),
        TranscriptImagePreviewState::Available
    );
    assert_eq!(
        fragment.backend_input(),
        &[
            UserInput::text("Look "),
            UserInput::text("Image A:"),
            UserInput::local_image(backend_path),
            UserInput::text(" now"),
        ]
    );

    cleanup_temp_dir(root);
}

#[test]
fn prepared_composer_draft_preserves_literal_marker_text_around_image_atom() {
    let (root, persistence, workspace_id, image_data) = durable_png_data(b"image");
    let mut draft = ComposerDraft::default();
    draft.sync_display_text("literal [A]  done".to_string());
    draft.replace_range_with_image("literal [A] ".len().."literal [A] ".len(), "A", image_data);

    let accepted = draft.accepted().expect("image draft should be accepted");
    let prepared_images = prepare_accepted_composer_images(
        &persistence,
        &workspace_id,
        &accepted,
        &RuntimeMode::HostWindows,
    )
    .expect("image should prepare for host runtime");
    let prepared = PreparedComposerDraft::new(accepted, prepared_images);
    let backend_path = prepared
        .backend_path_for_label("A")
        .expect("prepared image should have path")
        .to_string();

    let fragment =
        prepared_composer_draft_fragment(&prepared).expect("prepared draft should serialize");

    assert_eq!(fragment.text, "literal [A] [A] done");
    assert_eq!(
        fragment.backend_input(),
        &[
            UserInput::text("literal [A] "),
            UserInput::text("Image A:"),
            UserInput::local_image(backend_path),
            UserInput::text(" done"),
        ]
    );

    cleanup_temp_dir(root);
}

#[test]
fn repeated_image_references_serialize_one_attachment_and_later_text_reference() {
    let (root, persistence, workspace_id, image_data) = durable_png_data(b"image");
    let asset_id = image_data
        .asset_id()
        .expect("durable image data should carry asset id")
        .to_string();
    let mut draft = ComposerDraft::default();
    draft.sync_display_text(" then ".to_string());
    draft.replace_range_with_image(0..0, "A", image_data.clone());
    draft.replace_range_with_image(
        draft.display_text().len()..draft.display_text().len(),
        "A",
        image_data,
    );

    let accepted = draft.accepted().expect("image draft should be accepted");
    assert_eq!(accepted.images().count(), 1);
    let prepared_images = prepare_accepted_composer_images(
        &persistence,
        &workspace_id,
        &accepted,
        &RuntimeMode::HostWindows,
    )
    .expect("image should prepare for host runtime");
    let prepared = PreparedComposerDraft::new(accepted, prepared_images);
    let backend_path = prepared
        .backend_path_for_label("A")
        .expect("prepared image should have path")
        .to_string();

    let fragment =
        prepared_composer_draft_fragment(&prepared).expect("prepared draft should serialize");

    assert_eq!(fragment.text, "[A] then [A]");
    let markers = fragment.image_markers();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].display_range(), 0..3);
    assert_eq!(markers[1].display_range(), 9..12);
    assert_eq!(markers[0].source().asset_id(), Some(asset_id.as_str()));
    assert_eq!(markers[1].source().asset_id(), Some(asset_id.as_str()));
    assert_ne!(markers[0].occurrence_id(), markers[1].occurrence_id());
    assert_eq!(
        fragment.backend_input(),
        &[
            UserInput::text("Image A:"),
            UserInput::local_image(backend_path),
            UserInput::text(" then "),
            UserInput::text("[Image A]"),
        ]
    );

    cleanup_temp_dir(root);
}

fn durable_png_data(
    bytes: &[u8],
) -> (
    tempdir_support::TestTempDir,
    BerylWorkspacePersistence,
    BerylWorkspaceId,
    ComposerDraftImageData,
) {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id =
        BerylWorkspaceId::new("composer_submission").expect("workspace id should be valid");
    let asset = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, bytes)
        .expect("asset should be created");
    let image_data =
        ComposerDraftImageData::with_asset_id(ImageFormat::Png, bytes.to_vec(), asset.id());
    (root, persistence, workspace_id, image_data)
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-composer-submission-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    let _ = root.close();
}
