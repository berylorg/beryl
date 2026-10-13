#[test]
fn narrative_indentation_scales_with_body_and_inline_styles_compose() {
    use beryl_state::{ThemeDocument, ThemeParseMode, ThemeResolver};
    let document = ThemeDocument::parse_bytes(
        br#"
schema = 1
id = "narrative-scale"
name = "Narrative Scale"
[[role]]
id = "transcript.turn.assistant.final"
font_size = 28
[[role]]
id = "transcript-view.narrative-block.quote"
indent = 15
[[role]]
id = "transcript-view.narrative-block.list"
indent = 23
"#,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let resolved = ThemeResolver::new(document.definition()).unwrap().resolve();
    let appearance = NarrativeAppearance::prepare(&resolved);
    assert_eq!(appearance.block(NarrativeBlockStyle::Quote).indent, 30.0);
    assert_eq!(appearance.block(NarrativeBlockStyle::List).indent, 46.0);
    assert_eq!(
        appearance.block(NarrativeBlockStyle::Heading(1)).text.size,
        49.0
    );
    assert_eq!(
        appearance.block(NarrativeBlockStyle::Quote).border_width,
        2.0
    );
    let heading = appearance.block(NarrativeBlockStyle::Heading(2));
    let composed = appearance.inline(
        &heading,
        NarrativeInlineStyle {
            emphasis: true,
            strong: true,
            strikethrough: true,
            code: true,
            link_label: true,
        },
    );
    assert_eq!(composed.size, 42.0);
    assert_eq!(composed.font.style, FontStyle::Italic);
    assert_eq!(composed.font.weight, FontWeight(700.0));
    assert_eq!(composed.font.family.as_ref(), "monospace");
    assert!(composed.strikethrough);
    assert_eq!(composed.color, rgb(0x7dd3fc));
}
