use std::ops::Range;

pub(crate) fn vertical_hit_candidate_range<T, Y>(
    items_sorted_by_top: &[T],
    y: Y,
    top: impl Fn(&T) -> Y,
    bottom: impl Fn(&T) -> Y,
) -> Range<usize>
where
    Y: Copy + Ord,
{
    let upper = items_sorted_by_top.partition_point(|item| top(item) <= y);
    let start = items_sorted_by_top[..upper]
        .iter()
        .rposition(|item| bottom(item) < y)
        .map_or(0, |index| index.saturating_add(1));

    start..upper
}
