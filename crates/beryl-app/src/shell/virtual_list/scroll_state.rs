use super::*;

impl StateInner {
    pub(super) fn current_visible_range(&self) -> Range<usize> {
        self.current_range_with_overdraw(px(0.0))
    }

    pub(super) fn current_presentation_range(&self) -> Range<usize> {
        self.current_range_with_overdraw(self.overdraw)
    }

    fn current_range_with_overdraw(&self, overdraw: Pixels) -> Range<usize> {
        let count = self.items.summary().count;
        let Some(bounds) = self.last_layout_bounds else {
            return count..count;
        };
        if count == 0 {
            return 0..0;
        }

        let padding = self.last_padding.unwrap_or_default();
        let scroll_top = self.scroll_top_for_position(bounds.size.height, &padding);
        let range_start = (scroll_top - overdraw).max(px(0.0));
        let range_end = (scroll_top + bounds.size.height + overdraw).max(range_start);
        self.range_for_pixel_span(range_start, range_end)
    }

    fn range_for_pixel_span(&self, start_y: Pixels, end_y: Pixels) -> Range<usize> {
        let count = self.items.summary().count;
        if count == 0 || end_y <= start_y {
            return 0..0;
        }

        let start_offset = self.list_offset_for_scroll_top(start_y);
        let start = start_offset.item_ix.min(count);
        if start == count {
            return count..count;
        }

        let mut cursor = self.items.cursor::<ListItemSummary>(());
        cursor.seek(&Count(start), Bias::Right);
        cursor.seek_forward(&Height(end_y), Bias::Left);
        let end_exclusive = (cursor.start().count + 1).min(count).max(start);
        start..end_exclusive
    }

    pub(super) fn scroll(
        &mut self,
        scroll_top: &ListOffset,
        height: Pixels,
        delta: Point<Pixels>,
        current_view: EntityId,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Drop scroll events after a reset, since we can't calculate
        // the new logical scroll top without the item heights
        if self.reset {
            return;
        }

        let padding = self.last_padding.unwrap_or_default();
        let new_scroll_top = (self.scroll_top(scroll_top) - delta.y)
            .max(px(0.))
            .min(self.effective_scroll_max(height, &padding));
        self.set_scroll_position_from_scroll_top(new_scroll_top, height, &padding);

        if self.scroll_handler.is_some() {
            let visible_range = self.current_visible_range();
            let is_scrolled = self.is_scrolled(height, &padding);
            self.scroll_handler.as_mut().unwrap()(
                &ListScrollEvent {
                    visible_range,
                    count: self.items.summary().count,
                    is_scrolled,
                },
                window,
                cx,
            );
        }

        cx.notify(current_view);
    }

    pub(super) fn logical_scroll_top(&self) -> ListOffset {
        let bounds = self.last_layout_bounds.unwrap_or_default();
        let padding = self.last_padding.unwrap_or_default();
        match self.scroll_position() {
            ListScrollPosition::Bottom if self.alignment == ListAlignment::Bottom => ListOffset {
                item_ix: self.items.summary().count,
                offset_in_item: px(0.),
            },
            _ => self.list_offset_for_scroll_top(
                self.scroll_top_for_position(bounds.size.height, &padding),
            ),
        }
    }

    pub(super) fn scroll_position(&self) -> ListScrollPosition {
        let position = self
            .scroll_position
            .normalized()
            .limit_virtual_tail(self.virtual_trailing_scroll_allowance);
        match position {
            ListScrollPosition::VirtualTail {
                offset_from_content_end,
            } if offset_from_content_end == px(0.) => {
                let bounds = self.last_layout_bounds.unwrap_or_default();
                let padding = self.last_padding.unwrap_or_default();
                ListScrollPosition::Content(
                    self.list_offset_for_scroll_top(
                        self.real_scroll_max(bounds.size.height, &padding),
                    ),
                )
            }
            position => position,
        }
    }

    pub(super) fn is_scrolled(&self, height: Pixels, padding: &Edges<Pixels>) -> bool {
        match self.alignment {
            ListAlignment::Top => self.scroll_top_for_position(height, padding) > px(0.),
            ListAlignment::Bottom => !matches!(self.scroll_position(), ListScrollPosition::Bottom),
        }
    }

    pub(super) fn scroll_top(&self, logical_scroll_top: &ListOffset) -> Pixels {
        let (start, ..) = self.items.find::<ListItemSummary, _>(
            (),
            &Count(logical_scroll_top.item_ix),
            Bias::Right,
        );
        start.height + logical_scroll_top.offset_in_item
    }

    pub(super) fn list_offset_for_scroll_top(&self, scroll_top: Pixels) -> ListOffset {
        if scroll_top <= px(0.) {
            return ListOffset {
                item_ix: 0,
                offset_in_item: scroll_top,
            };
        }

        let (start, ..) =
            self.items
                .find::<ListItemSummary, _>((), &Height(scroll_top), Bias::Right);
        ListOffset {
            item_ix: start.count,
            offset_in_item: scroll_top - start.height,
        }
    }

    pub(super) fn real_scroll_max(&self, height: Pixels, padding: &Edges<Pixels>) -> Pixels {
        (self.items.summary().height + padding.top + padding.bottom - height).max(px(0.))
    }

    pub(super) fn effective_scroll_max(&self, height: Pixels, padding: &Edges<Pixels>) -> Pixels {
        (self.items.summary().height
            + padding.top
            + padding.bottom
            + self.virtual_trailing_scroll_allowance
            - height)
            .max(px(0.))
    }

    pub(super) fn visible_virtual_trailing_height(
        &self,
        height: Pixels,
        padding: &Edges<Pixels>,
        scroll_top: &ListOffset,
    ) -> Pixels {
        if matches!(self.scroll_position(), ListScrollPosition::Bottom) {
            return px(0.);
        }

        let content_height = self.items.summary().height + padding.top + padding.bottom;
        (self.scroll_top(scroll_top) + height - content_height)
            .max(px(0.))
            .min(self.virtual_trailing_scroll_allowance)
    }

    pub(super) fn scroll_top_for_position(
        &self,
        height: Pixels,
        padding: &Edges<Pixels>,
    ) -> Pixels {
        let real_scroll_max = self.real_scroll_max(height, padding);
        let effective_scroll_max = self.effective_scroll_max(height, padding);
        match self.scroll_position() {
            ListScrollPosition::Bottom if self.alignment == ListAlignment::Bottom => {
                real_scroll_max
            }
            ListScrollPosition::Bottom => px(0.),
            ListScrollPosition::Content(offset) => {
                self.scroll_top(&offset).clamp(px(0.), effective_scroll_max)
            }
            ListScrollPosition::VirtualTail {
                offset_from_content_end,
            } => (real_scroll_max + offset_from_content_end).min(effective_scroll_max),
        }
    }

    pub(super) fn set_scroll_position_from_scroll_top(
        &mut self,
        scroll_top: Pixels,
        height: Pixels,
        padding: &Edges<Pixels>,
    ) {
        let real_scroll_max = self.real_scroll_max(height, padding);
        let effective_scroll_max = self.effective_scroll_max(height, padding);
        let scroll_top = scroll_top.max(px(0.)).min(effective_scroll_max);
        let virtual_tail_range_active = effective_scroll_max > real_scroll_max;
        if scroll_top > real_scroll_max {
            self.scroll_position = ListScrollPosition::VirtualTail {
                offset_from_content_end: scroll_top - real_scroll_max,
            };
        } else if self.alignment == ListAlignment::Bottom
            && scroll_top == real_scroll_max
            && !virtual_tail_range_active
        {
            self.scroll_position = ListScrollPosition::Bottom;
        } else {
            self.scroll_position =
                ListScrollPosition::Content(self.list_offset_for_scroll_top(scroll_top));
        }
    }

    pub(super) fn set_virtual_trailing_scroll_allowance(&mut self, allowance: Pixels) {
        self.virtual_trailing_scroll_allowance = allowance.max(px(0.));
        self.scroll_position = self.scroll_position();
    }

    pub(super) fn scrollbar_content_height(&self) -> Pixels {
        self.items.summary().height + self.virtual_trailing_scroll_allowance
    }
}
