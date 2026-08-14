use super::BlockRenderListKind;

pub(crate) const MARKDOWN_LIST_LEADING_MARGIN_M: f32 = 1.5;
pub(crate) const MARKDOWN_LIST_MARKER_BODY_GAP_M: f32 = 0.5;
pub(crate) const MARKDOWN_LIST_UNORDERED_MARKER_WIDTH_M: f32 = 0.5;
pub(crate) const MARKDOWN_LIST_ORDERED_MARKER_CHARACTER_WIDTH_M: f32 = 0.75;

pub(crate) fn markdown_list_marker_width_m<I>(
    kind: BlockRenderListKind,
    marker_char_counts: I,
) -> f32
where
    I: IntoIterator<Item = usize>,
{
    match kind {
        BlockRenderListKind::Unordered => MARKDOWN_LIST_UNORDERED_MARKER_WIDTH_M,
        BlockRenderListKind::Ordered { .. } => marker_char_counts
            .into_iter()
            .map(|char_count| char_count as f32 * MARKDOWN_LIST_ORDERED_MARKER_CHARACTER_WIDTH_M)
            .fold(0.0, f32::max),
    }
}
