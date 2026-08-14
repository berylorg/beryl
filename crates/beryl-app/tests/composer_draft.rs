#![allow(dead_code)]

#[path = "../src/shell/composer_draft.rs"]
mod composer_draft;

use composer_draft::{
    AcceptedComposerDraftPart, COMPOSER_DRAFT_MAX_IMAGE_BYTES, COMPOSER_DRAFT_MAX_IMAGES,
    ComposerDraft, ComposerDraftImageAdmissionError, ComposerDraftImageAtom,
    ComposerDraftImageData, composer_image_label_from_atom_id, first_clipboard_image,
};
use gpui::{ClipboardItem, Image, ImageFormat};

fn png(bytes: &[u8]) -> ComposerDraftImageData {
    ComposerDraftImageData::new(ImageFormat::Png, bytes.to_vec())
}

fn image_atom(label: &str, text: &str) -> ComposerDraftImageAtom {
    let marker = format!("[{label}]");
    let start = text.find(&marker).expect("test text should contain marker");
    ComposerDraftImageAtom::new(label.to_string(), start..start + marker.len())
}

fn image_atom_with_id(
    label: &str,
    atom_id: &str,
    text: &str,
    occurrence: usize,
) -> ComposerDraftImageAtom {
    let marker = format!("[{label}]");
    let start = text
        .match_indices(&marker)
        .nth(occurrence)
        .map(|(index, _)| index)
        .expect("test text should contain marker occurrence");
    ComposerDraftImageAtom::new_with_atom_id(
        atom_id.to_string(),
        label.to_string(),
        start..start + marker.len(),
    )
}

#[test]
fn composer_draft_empty_detection_treats_text_and_image_markers_as_content() {
    let mut draft = ComposerDraft::default();
    assert!(draft.is_empty());

    draft.sync_display_text("plain text");
    assert!(!draft.is_empty());

    draft.sync_display_text("   ");
    assert!(!draft.is_empty());

    draft.clear();
    draft.sync_display_text("".to_string());
    draft.replace_range_with_image(0..0, "A", png(b"image"));
    assert!(!draft.is_empty());

    draft.sync_display_text("with text");
    draft.replace_range_with_image(4..4, "B", png(b"image"));
    assert!(!draft.is_empty());
}

#[test]
fn pasted_images_use_supplied_stable_labels_and_markers_in_display_text() {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text("Here  and ".to_string());

    let first = draft.replace_range_with_image("Here ".len().."Here ".len(), "A", png(b"one"));
    assert_eq!(first.marker(), "[A]");
    assert_eq!(draft.display_text(), "Here [A] and ");

    let second = draft.replace_range_with_image(
        draft.display_text().len()..draft.display_text().len(),
        "B",
        png(b"two"),
    );
    assert_eq!(second.marker(), "[B]");
    assert_eq!(draft.display_text(), "Here [A] and [B]");

    draft.sync_display_text(draft.display_text().replace("[A]", ""));
    let third = draft.replace_range_with_image(
        draft.display_text().len()..draft.display_text().len(),
        "C",
        png(b"three"),
    );
    assert_eq!(third.marker(), "[C]");
    assert_eq!(draft.display_text(), "Here  and [B][C]");
}

#[test]
fn accepted_draft_preserves_text_image_order_without_marker_text() {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text("Take a look: , please".to_string());
    draft.replace_range_with_image(
        "Take a look: ".len().."Take a look: ".len(),
        "A",
        png(b"shot"),
    );

    let accepted = draft
        .accepted()
        .expect("draft with image should be accepted");

    assert!(accepted.contains_images());
    assert_eq!(accepted.text_only(), None);
    let parts = accepted.parts();
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts[0],
        AcceptedComposerDraftPart::Text("Take a look: ".to_string())
    );
    match &parts[1] {
        AcceptedComposerDraftPart::Image(image) => {
            assert_eq!(image.label(), "A");
            assert_eq!(image.data(), &png(b"shot"));
        }
        AcceptedComposerDraftPart::Text(_) => panic!("expected image part"),
    }
    assert_eq!(
        parts[2],
        AcceptedComposerDraftPart::Text(", please".to_string())
    );
}

#[test]
fn accepted_draft_preserves_durable_image_asset_id_for_repeated_references() {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text(" then ".to_string());
    draft.replace_range_with_image(
        0..0,
        "A",
        ComposerDraftImageData::with_asset_id(ImageFormat::Png, b"image".to_vec(), "asset_a"),
    );
    draft.replace_range_with_image(
        draft.display_text().len()..draft.display_text().len(),
        "A",
        ComposerDraftImageData::with_asset_id(ImageFormat::Png, b"image".to_vec(), "asset_a"),
    );

    let accepted = draft.accepted().expect("image draft should be accepted");

    assert_eq!(accepted.image_asset_ids(), vec!["asset_a".to_string()]);
    let image = accepted
        .images()
        .next()
        .expect("one image should be emitted");
    assert_eq!(image.data().asset_id(), Some("asset_a"));
}

#[test]
fn literal_marker_text_is_not_an_image_without_an_atom() {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text("literal [A] text".to_string());

    let accepted = draft.accepted().expect("text draft should be accepted");

    assert!(!accepted.contains_images());
    assert_eq!(
        accepted.parts(),
        &[AcceptedComposerDraftPart::Text(
            "literal [A] text".to_string()
        )]
    );
}

#[test]
fn literal_marker_text_does_not_hijack_image_atom_order_or_removal() {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text("literal [A]  done".to_string());
    draft.replace_range_with_image(
        "literal [A] ".len().."literal [A] ".len(),
        "A",
        png(b"shot"),
    );
    assert_eq!(draft.display_text(), "literal [A] [A] done");

    let accepted = draft
        .accepted()
        .expect("draft with image atom should be accepted");
    let parts = accepted.parts();
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts[0],
        AcceptedComposerDraftPart::Text("literal [A] ".to_string())
    );
    match &parts[1] {
        AcceptedComposerDraftPart::Image(image) => {
            assert_eq!(image.label(), "A");
            assert_eq!(image.data(), &png(b"shot"));
        }
        AcceptedComposerDraftPart::Text(_) => panic!("expected image atom"),
    }
    assert_eq!(
        parts[2],
        AcceptedComposerDraftPart::Text(" done".to_string())
    );

    assert!(draft.remove_image_by_label("A"));
    assert_eq!(draft.display_text(), "literal [A]  done");
    assert_eq!(
        draft.accepted().unwrap().text_only(),
        Some("literal [A]  done".to_string())
    );
}

#[test]
fn input_sync_uses_only_explicit_atom_ranges() {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text("Look  now".to_string());
    draft.replace_range_with_image("Look ".len().."Look ".len(), "A", png(b"shot"));
    assert!(draft.accepted().unwrap().contains_images());

    draft.sync_from_input("Look [A] now".to_string(), Vec::new());

    let accepted = draft
        .accepted()
        .expect("literal marker text should be text");
    assert!(!accepted.contains_images());
    assert_eq!(accepted.text_only(), Some("Look [A] now".to_string()));
    assert_eq!(draft.image_data_for_label("A"), None);

    let mut draft = ComposerDraft::default();
    draft
        .stage_image("A", png(b"shot"))
        .expect("fixture image should fit");
    draft.sync_from_input(
        "Look [A] now".to_string(),
        vec![image_atom("A", "Look [A] now")],
    );
    assert!(draft.accepted().unwrap().contains_images());
}

#[test]
fn input_sync_allows_duplicate_labels_with_distinct_atom_ids() {
    let mut draft = ComposerDraft::default();
    let insertion = draft
        .stage_image("A", png(b"shot"))
        .expect("fixture image should fit");
    let display_text = "first [A], second [A]".to_string();
    draft.sync_from_input(
        display_text.clone(),
        vec![
            image_atom_with_id("A", insertion.atom_id(), &display_text, 0),
            image_atom_with_id("A", "composer-image:A:99", &display_text, 1),
        ],
    );

    assert_eq!(draft.image_labels(), vec!["A".to_string(), "A".to_string()]);
    assert_eq!(draft.image_data_for_label("A"), Some(&png(b"shot")));
    let accepted = draft.accepted().unwrap();
    assert_eq!(accepted.images().count(), 1);
    let image = accepted
        .images()
        .next()
        .expect("first image should be accepted")
        .clone();
    assert_eq!(
        accepted.parts(),
        &[
            AcceptedComposerDraftPart::Text("first ".to_string()),
            AcceptedComposerDraftPart::Image(image),
            AcceptedComposerDraftPart::Text(", second ".to_string()),
            AcceptedComposerDraftPart::Text("[Image A]".to_string()),
        ]
    );
}

#[test]
fn input_sync_rejects_duplicate_atom_ids_and_prunes_orphaned_images() {
    let mut draft = ComposerDraft::default();
    let insertion = draft
        .stage_image("A", png(b"shot"))
        .expect("fixture image should fit");
    let atom_id = insertion.atom_id().to_string();
    let display_text = "first [A], second [A]".to_string();
    draft.sync_from_input(
        display_text.clone(),
        vec![
            image_atom_with_id("A", &atom_id, &display_text, 0),
            image_atom_with_id("A", &atom_id, &display_text, 1),
        ],
    );

    assert_eq!(draft.image_labels(), vec!["A".to_string()]);
    assert_eq!(draft.image_data_for_label("A"), Some(&png(b"shot")));

    draft.sync_from_input(display_text, Vec::new());

    assert_eq!(draft.image_labels(), Vec::<String>::new());
    assert_eq!(draft.image_data_for_label("A"), None);
    assert_eq!(
        draft.accepted().unwrap().text_only(),
        Some("first [A], second [A]".to_string())
    );
}

#[test]
fn image_only_draft_is_accepted_and_blank_text_only_draft_is_rejected() {
    let mut image_draft = ComposerDraft::default();
    image_draft.replace_range_with_image(0..0, "A", png(b"only"));

    let accepted = image_draft
        .accepted()
        .expect("image-only draft should be accepted");
    assert!(accepted.contains_images());
    assert_eq!(accepted.parts().len(), 1);

    let mut blank = ComposerDraft::default();
    blank.sync_display_text(" \n\t ".to_string());
    assert_eq!(blank.accepted(), None);
}

#[test]
fn removing_image_by_label_removes_marker_and_keeps_later_labels_stable() {
    let mut draft = ComposerDraft::default();
    draft.replace_range_with_image(0..0, "A", png(b"one"));
    draft.replace_range_with_image(
        draft.display_text().len()..draft.display_text().len(),
        "B",
        png(b"two"),
    );

    assert_eq!(draft.image_labels(), vec!["A".to_string(), "B".to_string()]);
    assert!(draft.has_active_image_marker("A"));
    assert!(draft.has_active_image_marker("B"));

    assert!(draft.remove_image_by_label("A"));

    assert_eq!(draft.display_text(), "[B]");
    assert_eq!(draft.image_labels(), vec!["B".to_string()]);
    assert_eq!(draft.image_data_for_label("A"), None);
    assert_eq!(draft.image_data_for_label("B"), Some(&png(b"two")));

    let third = draft.replace_range_with_image(
        draft.display_text().len()..draft.display_text().len(),
        "C",
        png(b"three"),
    );
    assert_eq!(third.marker(), "[C]");
    assert_eq!(draft.display_text(), "[B][C]");
}

#[test]
fn removing_image_atom_by_id_removes_only_that_occurrence_until_final_reference() {
    let mut draft = ComposerDraft::default();
    let first = draft.replace_range_with_image(0..0, "A", png(b"shot"));
    let first_atom_id = first.atom_id().to_string();
    let second = draft.replace_range_with_image(
        draft.display_text().len()..draft.display_text().len(),
        "A",
        png(b"shot"),
    );
    let second_atom_id = second.atom_id().to_string();

    assert_ne!(first_atom_id, second_atom_id);
    assert_eq!(composer_image_label_from_atom_id(&first_atom_id), Some("A"));
    assert_eq!(
        composer_image_label_from_atom_id(&second_atom_id),
        Some("A")
    );
    assert_eq!(draft.display_text(), "[A][A]");
    assert_eq!(draft.image_labels(), vec!["A".to_string(), "A".to_string()]);
    assert_eq!(draft.image_data_for_label("A"), Some(&png(b"shot")));

    assert!(draft.remove_image_atom_by_id(&first_atom_id));
    assert_eq!(draft.display_text(), "[A]");
    assert_eq!(draft.image_labels(), vec!["A".to_string()]);
    assert_eq!(draft.image_data_for_label("A"), Some(&png(b"shot")));

    assert!(draft.remove_image_atom_by_id(&second_atom_id));
    assert_eq!(draft.display_text(), "");
    assert_eq!(draft.image_labels(), Vec::<String>::new());
    assert_eq!(draft.image_data_for_label("A"), None);
}

#[test]
fn clipboard_image_extraction_uses_image_entries_before_text_projection() {
    let image = Image::from_bytes(ImageFormat::Png, b"image".to_vec());
    let item = ClipboardItem::new_image(&image);

    assert_eq!(first_clipboard_image(&item), Some(image));
    assert_eq!(
        first_clipboard_image(&ClipboardItem::new_string("plain".to_string())),
        None
    );
}

#[test]
fn composer_draft_retained_counts_include_text_atoms_and_image_bytes() {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text("See [A]".to_string());
    draft
        .ensure_image_payload(
            "A",
            ComposerDraftImageData::with_asset_id(ImageFormat::Png, b"image".to_vec(), "asset-a"),
        )
        .expect("fixture image should fit");
    draft.sync_from_input("See [A]", vec![image_atom("A", "See [A]")]);

    let counts = draft.retained_counts();

    assert_eq!(counts.display_text_bytes, "See [A]".len());
    assert_eq!(counts.image_count, 1);
    assert_eq!(counts.image_bytes, b"image".len());
    assert_eq!(counts.image_asset_id_bytes, "asset-a".len());
    assert_eq!(counts.atom_count, 1);
    assert!(counts.atom_bytes >= "A".len() + "[A]".len());

    let accepted = draft.accepted().expect("draft has image content");
    let accepted_counts = accepted.retained_counts();
    assert_eq!(accepted_counts.image_count, 1);
    assert_eq!(accepted_counts.image_bytes, b"image".len());
    assert_eq!(accepted_counts.occurrence_count, 1);
    assert_eq!(accepted_counts.display_text_bytes, "See [A]".len());
}

#[test]
fn composer_draft_rejects_images_over_count_or_byte_budget() {
    let mut draft = ComposerDraft::default();
    for index in 0..COMPOSER_DRAFT_MAX_IMAGES {
        let label = format!("A{index}");
        draft
            .stage_image(label, png(b"tiny"))
            .expect("fixture images should fit until count limit");
    }

    assert_eq!(
        draft.stage_image("OVER", png(b"tiny")),
        Err(ComposerDraftImageAdmissionError::TooManyImages {
            limit: COMPOSER_DRAFT_MAX_IMAGES
        })
    );

    let mut draft = ComposerDraft::default();
    assert_eq!(
        draft.stage_image(
            "A",
            ComposerDraftImageData::new(
                ImageFormat::Png,
                vec![0; COMPOSER_DRAFT_MAX_IMAGE_BYTES + 1],
            ),
        ),
        Err(ComposerDraftImageAdmissionError::TooManyImageBytes {
            limit: COMPOSER_DRAFT_MAX_IMAGE_BYTES,
            attempted: COMPOSER_DRAFT_MAX_IMAGE_BYTES + 1,
        })
    );
}

#[test]
fn accepted_draft_can_drop_retained_bytes_for_durable_image_references() {
    let mut draft = ComposerDraft::default();
    draft.replace_range_with_image(
        0..0,
        "A",
        ComposerDraftImageData::with_asset_id(ImageFormat::Png, b"image".to_vec(), "asset-a"),
    );
    let accepted = draft.accepted().expect("image draft should be accepted");

    let compacted = accepted.with_durable_image_references();

    assert_eq!(accepted.retained_counts().image_bytes, b"image".len());
    assert_eq!(compacted.retained_counts().image_bytes, 0);
    assert_eq!(compacted.image_asset_ids(), vec!["asset-a".to_string()]);
}
