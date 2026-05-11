use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn word_range_at(text: &str, offset: usize) -> Option<Range<usize>> {
    if text.is_empty() {
        return None;
    }

    let offset = clamp_to_char_boundary(text, offset);
    let lookup = if offset >= text.len() {
        previous_grapheme_boundary(text, text.len())
    } else {
        offset
    };

    text.split_word_bound_indices()
        .filter_map(|(start, segment)| {
            let end = start + segment.len();
            (!segment.is_empty() && !segment.chars().all(char::is_whitespace)).then_some(start..end)
        })
        .find(|range| lookup >= range.start && lookup < range.end)
}

fn previous_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(text, offset);
    text[..offset]
        .grapheme_indices(true)
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
