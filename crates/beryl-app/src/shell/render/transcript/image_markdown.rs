use crate::shell::execution_detail::TranscriptImageMarker;

pub(super) fn markdown_source_with_image_marker_placeholders(
    text: &str,
    markers: &[TranscriptImageMarker],
) -> String {
    if markers.is_empty() {
        return text.to_string();
    }

    let mut markers = markers.iter().collect::<Vec<_>>();
    markers.sort_by_key(|marker| marker.display_range().start);

    let mut source = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for marker in markers {
        let range = marker.display_range();
        if range.start < cursor
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }

        let marker_text = format!("[{}]", marker.label());
        if text.get(range.clone()) != Some(marker_text.as_str()) {
            continue;
        }

        source.push_str(&text[cursor..range.start]);
        source.push('{');
        source.push_str(marker.label());
        source.push('}');
        cursor = range.end;
    }
    source.push_str(&text[cursor..]);
    source
}
