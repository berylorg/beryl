#![allow(dead_code)]

use std::collections::HashMap;

#[path = "../src/shell/composer_draft.rs"]
mod composer_draft;

#[path = "../src/shell/composer_clipboard.rs"]
mod composer_clipboard;

mod text_input {
    pub(crate) use gpui_text_input::TextInputSelectionAtom;
}

use composer_clipboard::{
    ComposerClipboardAtom, ComposerClipboardImage, ComposerClipboardLabelScope,
    ComposerClipboardPastePlan, ComposerClipboardPastePlanError, ComposerClipboardPayload,
    ComposerClipboardPayloadError, ComposerClipboardStore,
};
use composer_draft::{ComposerDraft, ComposerDraftImageAdmissionError, ComposerDraftImageData};
use gpui::{ClipboardItem, ImageFormat};

fn png(bytes: &[u8]) -> ComposerDraftImageData {
    ComposerDraftImageData::new(ImageFormat::Png, bytes.to_vec())
}

fn payload_for_label(label: &str, bytes: &[u8]) -> ComposerClipboardPayload {
    let marker = format!("[{label}]");
    let copy_text = format!("[Image {label}]");
    ComposerClipboardPayload::new(
        format!("See {marker}"),
        format!("See {copy_text}"),
        ComposerClipboardLabelScope::PendingNewThread(1),
        vec![ComposerClipboardAtom::new(
            label.to_string(),
            4..4 + marker.len(),
            marker,
            copy_text,
        )],
        vec![ComposerClipboardImage::new(label.to_string(), png(bytes))],
    )
    .expect("test payload should be valid")
}

fn durable_payload_for_label(
    label: &str,
    bytes: &[u8],
    asset_id: &str,
) -> ComposerClipboardPayload {
    let marker = format!("[{label}]");
    let copy_text = format!("[Image {label}]");
    ComposerClipboardPayload::new(
        format!("See {marker}"),
        format!("See {copy_text}"),
        ComposerClipboardLabelScope::PendingNewThread(1),
        vec![ComposerClipboardAtom::new(
            label.to_string(),
            4..4 + marker.len(),
            marker,
            copy_text,
        )],
        vec![ComposerClipboardImage::new(
            label.to_string(),
            ComposerDraftImageData::with_asset_id(ImageFormat::Png, bytes.to_vec(), asset_id),
        )],
    )
    .expect("test payload should be valid")
}

fn repeated_payload_for_label(label: &str, bytes: &[u8]) -> ComposerClipboardPayload {
    let marker = format!("[{label}]");
    let copy_text = format!("[Image {label}]");
    ComposerClipboardPayload::new(
        format!("See {marker} and {marker}"),
        format!("See {copy_text} and {copy_text}"),
        ComposerClipboardLabelScope::PendingNewThread(1),
        vec![
            ComposerClipboardAtom::new(
                label.to_string(),
                4..4 + marker.len(),
                marker.clone(),
                copy_text.clone(),
            ),
            ComposerClipboardAtom::new(
                label.to_string(),
                9 + marker.len()..9 + marker.len() * 2,
                marker,
                copy_text,
            ),
        ],
        vec![ComposerClipboardImage::new(label.to_string(), png(bytes))],
    )
    .expect("test payload should be valid")
}

#[test]
fn clipboard_store_retained_counts_include_payload_text_atoms_and_images() {
    let mut store = ComposerClipboardStore::default();
    store.store_payload(payload_for_label("A", b"image"));

    let counts = store.retained_counts();

    assert_eq!(counts.payloads, 1);
    assert_eq!(counts.tokens, 1);
    assert!(counts.token_bytes > 0);
    assert_eq!(counts.selected_text_bytes, "See [A]".len());
    assert_eq!(counts.fallback_text_bytes, "See [Image A]".len());
    assert_eq!(counts.atom_count, 1);
    assert!(counts.atom_bytes >= "A".len() + "[A]".len() + "[Image A]".len());
    assert_eq!(counts.image_count, 1);
    assert_eq!(counts.image_bytes, b"image".len());
}

#[test]
fn store_writes_metadata_token_and_resolves_live_matching_payload() {
    let payload = payload_for_label("A", b"secret-image-bytes");
    let mut store = ComposerClipboardStore::default();

    let item = store.store_payload(payload.clone());

    assert_eq!(item.text().as_deref(), Some("See [Image A]"));
    let metadata = item.metadata().expect("metadata should be present");
    assert!(metadata.contains("beryl.composer.image-selection"));
    assert!(metadata.contains("composer-clipboard-"));
    assert!(!metadata.contains("secret-image-bytes"));
    assert_eq!(store.resolve_payload(&item), Some(payload));
}

#[test]
fn resolve_rejects_stale_missing_malformed_and_tampered_metadata() {
    let payload = payload_for_label("A", b"image");
    let mut store = ComposerClipboardStore::default();
    let item = store.store_payload(payload);
    let metadata = item.metadata().expect("metadata should be present").clone();

    assert_eq!(
        ComposerClipboardStore::default().resolve_payload(&item),
        None
    );
    assert_eq!(
        store.resolve_payload(&ClipboardItem::new_string("See [Image A]".to_string())),
        None
    );
    assert_eq!(
        store.resolve_payload(&ClipboardItem::new_string_with_metadata(
            "See [Image A]".to_string(),
            "{\"marker\":\"beryl.composer.image-selection\",\"version\":1}".to_string(),
        )),
        None
    );
    assert_eq!(
        store.resolve_payload(&ClipboardItem::new_string_with_metadata(
            "See changed".to_string(),
            metadata,
        )),
        None
    );
}

#[test]
fn store_evicts_old_tokens_without_exposing_image_bytes_in_metadata() {
    let mut store = ComposerClipboardStore::with_capacity(1);
    let first = store.store_payload(payload_for_label("A", b"first-image"));
    let second_payload = payload_for_label("B", b"second-image");

    let second = store.store_payload(second_payload.clone());

    assert_eq!(store.resolve_payload(&first), None);
    assert_eq!(store.resolve_payload(&second), Some(second_payload));
    assert!(!second.metadata().unwrap().contains("second-image"));
}

#[test]
fn store_evicts_old_payloads_to_stay_under_image_byte_budget() {
    let mut store = ComposerClipboardStore::with_limits(8, 10);
    let first = store.store_payload(payload_for_label("A", b"123456"));
    let second_payload = payload_for_label("B", b"abcdef");
    let second = store.store_payload(second_payload.clone());

    assert_eq!(store.resolve_payload(&first), None);
    assert_eq!(store.resolve_payload(&second), Some(second_payload));
    assert_eq!(store.retained_counts().image_bytes, 6);

    let too_large = store.store_payload(payload_for_label("C", b"01234567890"));
    assert_eq!(store.resolve_payload(&too_large), None);
    assert_eq!(store.retained_counts().image_bytes, 0);
}

#[test]
fn payload_validation_requires_atoms_to_match_display_text_and_images() {
    let missing_image = ComposerClipboardPayload::new(
        "See [A]",
        "See [Image A]",
        ComposerClipboardLabelScope::Thread("thread_1".to_string()),
        vec![ComposerClipboardAtom::new("A", 4..7, "[A]", "[Image A]")],
        Vec::new(),
    );
    assert_eq!(
        missing_image,
        Err(ComposerClipboardPayloadError::MissingImageData)
    );

    let partial_marker = ComposerClipboardPayload::new(
        "See [A]",
        "See [Image A]",
        ComposerClipboardLabelScope::Thread("thread_1".to_string()),
        vec![ComposerClipboardAtom::new("A", 4..6, "[A", "[Image A]")],
        vec![ComposerClipboardImage::new("A", png(b"image"))],
    );
    assert_eq!(
        partial_marker,
        Err(ComposerClipboardPayloadError::InvalidAtomMarker)
    );
}

#[test]
fn paste_plan_preserves_same_scope_labels_and_repeated_image_payloads() {
    let payload = repeated_payload_for_label("A", b"image");
    let label_mapping = HashMap::from([("A".to_string(), "A".to_string())]);
    let mut ordinal = 0u64;

    let plan = ComposerClipboardPastePlan::new(&payload, &label_mapping, |label| {
        let atom_id = format!("composer-image:{label}:{ordinal}");
        ordinal += 1;
        atom_id
    })
    .expect("same-scope paste should plan");

    assert_eq!(plan.display_text(), "See [A] and [A]");
    assert_eq!(plan.atoms().len(), 2);
    assert_eq!(plan.atoms()[0].range(), 4..7);
    assert_eq!(plan.atoms()[1].range(), 12..15);
    assert_eq!(plan.atoms()[0].copy_text(), "[Image A]");
    assert_eq!(plan.atoms()[1].copy_text(), "[Image A]");
    assert_eq!(plan.images().len(), 1);
    assert_eq!(plan.images()[0].label(), "A");
    assert_eq!(plan.images()[0].data(), &png(b"image"));
}

#[test]
fn same_scope_paste_rejects_stale_payload_after_draft_label_reuse() {
    let old_payload = payload_for_label("A", b"old-image");
    let mut draft = ComposerDraft::default();
    let old = draft.replace_range_with_image(0..0, "A", png(b"old-image"));
    assert!(draft.remove_image_atom_by_id(old.atom_id()));
    draft.replace_range_with_image(0..0, "A", png(b"new-image"));
    let label_mapping = HashMap::from([("A".to_string(), "A".to_string())]);

    let plan = ComposerClipboardPastePlan::new(&old_payload, &label_mapping, |label| {
        format!("composer-image:{label}:stale")
    })
    .expect("same-scope paste plan can preserve the copied label");
    let mut admission_probe = draft.clone();

    assert_eq!(
        admission_probe.ensure_image_payload(
            plan.images()[0].label().to_string(),
            plan.images()[0].data().clone(),
        ),
        Err(
            ComposerDraftImageAdmissionError::ImageLabelPayloadMismatch {
                label: "A".to_string()
            }
        )
    );
    assert_eq!(draft.image_data_for_label("A"), Some(&png(b"new-image")));
}

#[test]
fn same_scope_paste_accepts_same_durable_asset_after_retained_bytes_are_compacted() {
    let payload = durable_payload_for_label("A", b"image-bytes", "asset-a");
    let mut draft = ComposerDraft::default();
    draft.replace_range_with_image(
        0..0,
        "A",
        ComposerDraftImageData::durable_reference(ImageFormat::Png, "asset-a"),
    );
    let label_mapping = HashMap::from([("A".to_string(), "A".to_string())]);

    let plan = ComposerClipboardPastePlan::new(&payload, &label_mapping, |label| {
        format!("composer-image:{label}:durable")
    })
    .expect("same-scope paste plan can preserve the copied label");
    let mut admission_probe = draft.clone();

    assert_eq!(
        admission_probe.ensure_image_payload(
            plan.images()[0].label().to_string(),
            plan.images()[0].data().clone(),
        ),
        Ok(false)
    );

    let mut mismatch_probe = draft.clone();
    assert_eq!(
        mismatch_probe.ensure_image_payload(
            "A",
            ComposerDraftImageData::with_asset_id(
                ImageFormat::Png,
                b"different".to_vec(),
                "asset-b"
            ),
        ),
        Err(
            ComposerDraftImageAdmissionError::ImageLabelPayloadMismatch {
                label: "A".to_string()
            }
        )
    );
    assert_eq!(
        draft.image_data_for_label("A"),
        Some(&ComposerDraftImageData::durable_reference(
            ImageFormat::Png,
            "asset-a"
        ))
    );
}

#[test]
fn paste_plan_remaps_cross_scope_labels_and_atom_ranges() {
    let payload = payload_for_label("Z", b"image");
    let label_mapping = HashMap::from([("Z".to_string(), "AA".to_string())]);

    let plan = ComposerClipboardPastePlan::new(&payload, &label_mapping, |label| {
        format!("composer-image:{label}:0")
    })
    .expect("cross-scope paste should plan");

    assert_eq!(plan.display_text(), "See [AA]");
    assert_eq!(plan.atoms().len(), 1);
    assert_eq!(plan.atoms()[0].range(), 4..8);
    assert_eq!(plan.atoms()[0].copy_text(), "[Image AA]");
    assert_eq!(plan.images().len(), 1);
    assert_eq!(plan.images()[0].label(), "AA");
}

#[test]
fn paste_plan_rejects_missing_duplicate_and_invalid_label_mappings() {
    let payload = ComposerClipboardPayload::new(
        "See [A] and [B]",
        "See [Image A] and [Image B]",
        ComposerClipboardLabelScope::Thread("thread_1".to_string()),
        vec![
            ComposerClipboardAtom::new("A", 4..7, "[A]", "[Image A]"),
            ComposerClipboardAtom::new("B", 12..15, "[B]", "[Image B]"),
        ],
        vec![
            ComposerClipboardImage::new("A", png(b"one")),
            ComposerClipboardImage::new("B", png(b"two")),
        ],
    )
    .expect("payload should be valid");

    assert_eq!(
        ComposerClipboardPastePlan::new(
            &payload,
            &HashMap::from([("A".to_string(), "C".to_string())]),
            |label| format!("composer-image:{label}:0"),
        ),
        Err(ComposerClipboardPastePlanError::MissingLabelMapping)
    );
    assert_eq!(
        ComposerClipboardPastePlan::new(
            &payload,
            &HashMap::from([
                ("A".to_string(), "C".to_string()),
                ("B".to_string(), "C".to_string()),
            ]),
            |label| format!("composer-image:{label}:0"),
        ),
        Err(ComposerClipboardPastePlanError::DuplicateTargetLabel)
    );
    assert_eq!(
        ComposerClipboardPastePlan::new(
            &payload,
            &HashMap::from([
                ("A".to_string(), "C".to_string()),
                ("B".to_string(), "not-a-label".to_string()),
            ]),
            |label| format!("composer-image:{label}:0"),
        ),
        Err(ComposerClipboardPastePlanError::InvalidTargetLabel)
    );
}
