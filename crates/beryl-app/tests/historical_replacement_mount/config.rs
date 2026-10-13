use gpui::{
    SharedString, StreamingLayoutBinding, StreamingLayoutLimits, StreamingLayoutPosition, TextRun,
    black, font, px,
};
use gpui_scrollbar::ScrollbarStyle;
use gpui_text_input::*;

pub fn widget(binding: gpui_text_input::RangeBinding) -> RangeTextInputConfig {
    let layout = StreamingLayoutBinding {
        input_id: 11,
        segment_policy_id: 13,
        start_position: StreamingLayoutPosition::at(0),
        wrap_width: px(320.),
        font_size: px(12.),
        line_height: px(16.),
        limits: StreamingLayoutLimits {
            segment_bytes: 32,
            runs: 8,
            decorations: 8,
            glyphs: 256,
            wraps: 128,
            maps: 257,
            fragments: 1,
            retained_items: 4096,
            retained_bytes: 256 * 1024,
        },
    };
    let run = TextRun {
        len: 0,
        font: font(".SystemUIFont"),
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    RangeTextInputConfig {
        binding,
        presentation_generation: PresentationGeneration::new(1),
        enter_key: TextInputEnterKey::Propagate,
        atom_clipboard_policy: TextInputAtomClipboardPolicy::Propagate,
        rich_paste_policy: TextInputRichPastePolicy::Propagate,
        layout,
        style: StreamingGeometryStyle::new(
            run,
            StreamingOversizePresentation::new(
                SharedString::new_static(""),
                vec![],
                px(12.),
                px(16.),
                px(12.),
                None,
            ),
        ),
        geometry_limits: ExactGeometryLimits::new(32, 8, 512 * 1024, 8192).unwrap(),
        residency_limits: ResidencyLimits::new(8, 128 * 1024, 8, 256).unwrap(),
        object_residency_limits: ObjectResidencyLimits::new(
            4,
            32,
            65_536,
            32 * 1024,
            4,
            32,
            65_536,
        )
        .unwrap(),
        mutation_limits: MutationLimits::new(8, 256).unwrap(),
        clipboard_limits: ClipboardLimits::new(1024, 32).unwrap(),
        segmentation_limits: SegmentationLimits::new(32, 64).unwrap(),
        limits: RangeTextInputLimits::new(2 * 1024 * 1024, 32768, 1, px(80.), 32, 32, px(16.))
            .unwrap(),
        settlement_coordinator: RangeSettlementCoordinator::new(4).unwrap(),
        viewport_extent: px(80.),
        overscan: px(32.),
        placeholder: SharedString::new_static("Message"),
        theme: TextInputTheme::default(),
        scrollbar_style: ScrollbarStyle::default(),
    }
}
