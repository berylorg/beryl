#[path = "../src/shell/transcript_media.rs"]
mod transcript_media;

use gpui::px;
use transcript_media::{
    TranscriptMediaLayoutInput, TranscriptMediaLoadOutcome, TranscriptMediaNaturalDimensions,
    TranscriptMediaRunAlignment, TranscriptMediaSizingInput, TranscriptMediaSlotLayout,
    transcript_media_layout_metrics, transcript_media_run_alignment, transcript_media_size,
    transcript_media_slot_layout,
};

#[test]
fn single_large_image_fits_content_width_and_preserves_aspect_ratio() {
    let size = transcript_media_size(input(1, 600.0, 10.0, Some(natural(1200, 800)), 1.0));

    assert_eq!(size.width, px(600.0));
    assert_eq!(size.height, px(400.0));
}

#[test]
fn single_small_image_is_not_upscaled_past_scale_adjusted_natural_width() {
    let size = transcript_media_size(input(1, 600.0, 10.0, Some(natural(300, 150)), 2.0));

    assert_eq!(size.width, px(150.0));
    assert_eq!(size.height, px(75.0));
}

#[test]
fn tall_and_wide_images_use_aspect_ratio_height() {
    let tall = transcript_media_size(input(1, 200.0, 10.0, Some(natural(100, 400)), 1.0));
    let wide = transcript_media_size(input(1, 200.0, 10.0, Some(natural(400, 100)), 1.0));

    assert_eq!(tall.width, px(100.0));
    assert_eq!(tall.height, px(400.0));
    assert_eq!(wide.width, px(200.0));
    assert_eq!(wide.height, px(50.0));
}

#[test]
fn multi_image_run_uses_thirty_m_advances_and_caps_each_small_image() {
    let large = transcript_media_size(input(3, 800.0, 8.0, Some(natural(900, 450)), 1.0));
    let small = transcript_media_size(input(3, 800.0, 8.0, Some(natural(120, 60)), 1.0));

    assert_eq!(large.width, px(240.0));
    assert_eq!(large.height, px(120.0));
    assert_eq!(small.width, px(120.0));
    assert_eq!(small.height, px(60.0));
}

#[test]
fn multi_image_target_wraps_within_narrow_content_width() {
    let size = transcript_media_size(input(2, 180.0, 8.0, Some(natural(900, 450)), 1.0));

    assert_eq!(size.width, px(180.0));
    assert_eq!(size.height, px(90.0));
}

#[test]
fn promoted_image_uses_single_image_sizing() {
    let compact = transcript_media_size(input(3, 800.0, 8.0, Some(natural(900, 450)), 1.0));
    let promoted = transcript_media_size(input(1, 800.0, 8.0, Some(natural(900, 450)), 1.0));

    assert_eq!(compact.width, px(240.0));
    assert_eq!(compact.height, px(120.0));
    assert_eq!(promoted.width, px(800.0));
    assert_eq!(promoted.height, px(400.0));
}

#[test]
fn non_promoted_single_sibling_keeps_compact_multi_image_sizing() {
    let sibling = transcript_media_size(input(2, 800.0, 8.0, Some(natural(900, 450)), 1.0));

    assert_eq!(sibling.width, px(240.0));
    assert_eq!(sibling.height, px(120.0));
}

#[test]
fn pending_placeholder_dimensions_are_stable_until_natural_dimensions_arrive() {
    let pending = transcript_media_size(input(1, 420.0, 8.0, None, 1.0));
    let loaded = transcript_media_size(input(1, 420.0, 8.0, Some(natural(840, 420)), 1.0));

    assert_eq!(pending.width, px(420.0));
    assert_eq!(pending.height, px(420.0));
    assert_eq!(loaded.width, px(420.0));
    assert_eq!(loaded.height, px(210.0));
}

#[test]
fn terminal_failures_use_text_fallback_instead_of_placeholder_sizing() {
    let outcomes = [
        (
            TranscriptMediaLoadOutcome::RenderNotSupported {
                alt: "vector".to_string(),
            },
            "vector (render not supported)",
        ),
        (
            TranscriptMediaLoadOutcome::FileUnavailable {
                alt: "missing".to_string(),
            },
            "missing (file unavailable)",
        ),
        (
            TranscriptMediaLoadOutcome::PathNotAllowed {
                alt: "secret plan".to_string(),
            },
            "secret plan (path not allowed)",
        ),
    ];

    for (outcome, expected) in outcomes {
        let layout = transcript_media_slot_layout(input(1, 420.0, 8.0, None, 1.0), Some(&outcome));

        assert_eq!(
            layout,
            TranscriptMediaSlotLayout::TextFallback(expected.to_string())
        );
    }
}

#[test]
fn pending_media_keeps_placeholder_sizing_until_load_finishes() {
    let outcome = TranscriptMediaLoadOutcome::Pending {
        alt: "diagram".to_string(),
    };

    let layout = transcript_media_slot_layout(input(1, 420.0, 8.0, None, 1.0), Some(&outcome));

    assert_eq!(
        layout,
        TranscriptMediaSlotLayout::Media(transcript_media_size(input(1, 420.0, 8.0, None, 1.0)))
    );
}

#[test]
fn media_layout_derives_padded_content_width_from_transcript_row_padding() {
    let layout = transcript_media_layout_metrics(TranscriptMediaLayoutInput {
        transcript_width: px(700.0),
        row_horizontal_padding: px(24.0),
        conversation_m_advance: px(9.0),
        window_scale: 1.25,
    });

    assert_eq!(layout.padded_content_width, px(676.0));
    assert_eq!(layout.conversation_m_advance, px(9.0));
    assert_eq!(layout.window_scale, 1.25);
}

#[test]
fn media_layout_keeps_active_conversation_font_metric_for_run_sizing() {
    let narrow_font = transcript_media_layout_metrics(TranscriptMediaLayoutInput {
        transcript_width: px(900.0),
        row_horizontal_padding: px(24.0),
        conversation_m_advance: px(7.0),
        window_scale: 1.0,
    });
    let wide_font = transcript_media_layout_metrics(TranscriptMediaLayoutInput {
        transcript_width: px(900.0),
        row_horizontal_padding: px(24.0),
        conversation_m_advance: px(11.0),
        window_scale: 1.0,
    });

    let narrow_size = transcript_media_size(TranscriptMediaSizingInput {
        run_length: 2,
        padded_content_width: narrow_font.padded_content_width,
        conversation_m_advance: narrow_font.conversation_m_advance,
        natural_dimensions: Some(natural(900, 450)),
        window_scale: narrow_font.window_scale,
    });
    let wide_size = transcript_media_size(TranscriptMediaSizingInput {
        run_length: 2,
        padded_content_width: wide_font.padded_content_width,
        conversation_m_advance: wide_font.conversation_m_advance,
        natural_dimensions: Some(natural(900, 450)),
        window_scale: wide_font.window_scale,
    });

    assert_eq!(narrow_size.width, px(210.0));
    assert_eq!(wide_size.width, px(330.0));
}

#[test]
fn single_media_run_alignment_centers_only_one_item() {
    assert_eq!(
        transcript_media_run_alignment(1),
        TranscriptMediaRunAlignment::Center
    );
    assert_eq!(
        transcript_media_run_alignment(0),
        TranscriptMediaRunAlignment::Start
    );
    assert_eq!(
        transcript_media_run_alignment(2),
        TranscriptMediaRunAlignment::Start
    );
}

fn input(
    run_length: usize,
    padded_content_width: f32,
    conversation_m_advance: f32,
    natural_dimensions: Option<TranscriptMediaNaturalDimensions>,
    window_scale: f32,
) -> TranscriptMediaSizingInput {
    TranscriptMediaSizingInput {
        run_length,
        padded_content_width: px(padded_content_width),
        conversation_m_advance: px(conversation_m_advance),
        natural_dimensions,
        window_scale,
    }
}

fn natural(width: u32, height: u32) -> TranscriptMediaNaturalDimensions {
    TranscriptMediaNaturalDimensions::new(width, height).expect("dimensions should be non-zero")
}
