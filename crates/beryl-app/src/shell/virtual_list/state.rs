use super::*;

impl ListState {
    /// Construct a new list state, for storage on a view.
    ///
    /// The overdraw parameter controls how much extra space is rendered
    /// above and below the visible area. Elements within this area will
    /// be measured even though they are not visible. This can help ensure
    /// that the list doesn't flicker or pop in when scrolling.
    pub fn new(item_count: usize, alignment: ListAlignment, overdraw: Pixels) -> Self {
        let this = Self(Rc::new(RefCell::new(StateInner {
            last_layout_bounds: None,
            last_padding: None,
            items: SumTree::default(),
            scroll_position: match alignment {
                ListAlignment::Top => ListScrollPosition::Content(ListOffset::default()),
                ListAlignment::Bottom => ListScrollPosition::Bottom,
            },
            virtual_trailing_scroll_allowance: px(0.),
            alignment,
            overdraw,
            scroll_handler: None,
            reset: false,
            scrollbar_drag_start_height: None,
            measuring_behavior: ListMeasuringBehavior::default(),
        })));
        this.splice(0..0, item_count);
        this.0.borrow_mut().scroll_position = match alignment {
            ListAlignment::Top => ListScrollPosition::Content(ListOffset::default()),
            ListAlignment::Bottom => ListScrollPosition::Bottom,
        };
        this
    }

    /// Set the list to measure all items in the list in the first layout phase.
    ///
    /// This is useful for ensuring that the scrollbar size is correct instead of based on only rendered elements.
    pub fn measure_all(self) -> Self {
        self.0.borrow_mut().measuring_behavior = ListMeasuringBehavior::Measure(false);
        self
    }

    /// Reset this instantiation of the list state.
    ///
    /// Note that this will cause scroll events to be dropped until the next paint.
    pub fn reset(&self, element_count: usize) {
        let old_count = {
            let state = &mut *self.0.borrow_mut();
            state.reset = true;
            state.measuring_behavior.reset();
            state.scroll_position = match state.alignment {
                ListAlignment::Top => ListScrollPosition::Content(ListOffset::default()),
                ListAlignment::Bottom => ListScrollPosition::Bottom,
            };
            state.scrollbar_drag_start_height = None;
            state.items.summary().count
        };

        self.splice(0..old_count, element_count);
        let state = &mut *self.0.borrow_mut();
        state.scroll_position = match state.alignment {
            ListAlignment::Top => ListScrollPosition::Content(ListOffset::default()),
            ListAlignment::Bottom => ListScrollPosition::Bottom,
        };
    }

    /// The number of items in this list.
    pub fn item_count(&self) -> usize {
        self.0.borrow().items.summary().count
    }

    /// The real-content item range currently intersecting the viewport.
    pub fn visible_range(&self) -> Range<usize> {
        self.0.borrow().current_visible_range()
    }

    /// The real-content item range needed for the current frame, including list overdraw.
    pub fn presentation_range(&self) -> Range<usize> {
        self.0.borrow().current_presentation_range()
    }

    /// The real-content item range intersecting the viewport plus an explicit vertical margin.
    pub fn range_with_vertical_margin(&self, margin: Pixels) -> Range<usize> {
        self.0.borrow().current_range_with_overdraw(margin)
    }

    /// Inform the list state that the items in `old_range` have been replaced
    /// by `count` new items that must be recalculated.
    pub fn splice(&self, old_range: Range<usize>, count: usize) {
        self.splice_focusable(old_range, (0..count).map(|_| None))
    }

    /// Register with the list state that the items in `old_range` have been replaced
    /// by new items. As opposed to [`Self::splice`], this method allows an iterator of optional focus handles
    /// to be supplied to properly integrate with items in the list that can be focused. If a focused item
    /// is scrolled out of view, the list will continue to render it to allow keyboard interaction.
    pub fn splice_focusable(
        &self,
        old_range: Range<usize>,
        focus_handles: impl IntoIterator<Item = Option<FocusHandle>>,
    ) {
        let state = &mut *self.0.borrow_mut();

        let mut old_items = state.items.cursor::<Count>(());
        let mut new_items = old_items.slice(&Count(old_range.start), Bias::Right);
        old_items.seek_forward(&Count(old_range.end), Bias::Right);

        let mut spliced_count = 0;
        new_items.extend(
            focus_handles.into_iter().map(|focus_handle| {
                spliced_count += 1;
                ListItem::Unmeasured { focus_handle }
            }),
            (),
        );
        new_items.append(old_items.suffix(), ());
        drop(old_items);
        state.items = new_items;

        if let ListScrollPosition::Content(ListOffset {
            item_ix,
            offset_in_item,
        }) = &mut state.scroll_position
        {
            if old_range.contains(item_ix) {
                *item_ix = old_range.start;
                *offset_in_item = px(0.);
            } else if old_range.end <= *item_ix {
                *item_ix = *item_ix - (old_range.end - old_range.start) + spliced_count;
            }
        }
    }

    /// Mark one item's cached measurement as stale while retaining its last known size
    /// for scroll geometry until the next layout pass can remeasure it.
    pub fn invalidate_item_measurement(&self, ix: usize) {
        let state = &mut *self.0.borrow_mut();
        if ix >= state.items.summary().count {
            return;
        }

        state.items = SumTree::from_iter(
            state
                .items
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, item)| {
                    if index != ix {
                        return item;
                    }
                    match item {
                        ListItem::Unmeasured { focus_handle } => {
                            ListItem::Unmeasured { focus_handle }
                        }
                        ListItem::Measured { size, focus_handle }
                        | ListItem::DirtyMeasured { size, focus_handle } => {
                            ListItem::DirtyMeasured { size, focus_handle }
                        }
                    }
                }),
            (),
        );
    }

    /// Set a handler that will be called when the list is scrolled.
    pub fn set_scroll_handler(
        &self,
        handler: impl FnMut(&ListScrollEvent, &mut Window, &mut App) + 'static,
    ) {
        self.0.borrow_mut().scroll_handler = Some(Box::new(handler))
    }

    /// Get the current scroll offset, in terms of the list's items.
    pub fn logical_scroll_top(&self) -> ListOffset {
        self.0.borrow().logical_scroll_top()
    }

    /// Get the current scroll position intent.
    pub fn scroll_position(&self) -> ListScrollPosition {
        self.0.borrow().scroll_position()
    }

    /// Scroll the list to the given position intent.
    pub fn scroll_to_position(&self, scroll_position: ListScrollPosition) {
        self.0.borrow_mut().scroll_position = scroll_position.normalized();
    }

    /// Set trailing virtual scroll allowance in pixels.
    ///
    /// This extends the scroll range after real content without adding list items.
    pub fn set_virtual_trailing_scroll_allowance(&self, allowance: Pixels) {
        self.0
            .borrow_mut()
            .set_virtual_trailing_scroll_allowance(allowance);
    }

    /// Get trailing virtual scroll allowance in pixels.
    pub fn virtual_trailing_scroll_allowance(&self) -> Pixels {
        self.0.borrow().virtual_trailing_scroll_allowance
    }

    /// Scroll the list by the given offset
    pub fn scroll_by(&self, distance: Pixels) {
        if distance == px(0.) {
            return;
        }

        let state = &mut *self.0.borrow_mut();
        let current_offset = state.logical_scroll_top();
        let start_pixel_offset = state.scroll_top(&current_offset);
        let new_pixel_offset = (start_pixel_offset + distance).max(px(0.));
        if state.virtual_trailing_scroll_allowance > px(0.)
            && let Some(bounds) = state.last_layout_bounds
        {
            let padding = state.last_padding.unwrap_or_default();
            state.set_scroll_position_from_scroll_top(
                new_pixel_offset.min(state.effective_scroll_max(bounds.size.height, &padding)),
                bounds.size.height,
                &padding,
            );
        } else {
            state.scroll_position =
                ListScrollPosition::Content(state.list_offset_for_scroll_top(new_pixel_offset));
        }
    }

    /// Scroll the list to the given offset
    pub fn scroll_to(&self, mut scroll_top: ListOffset) {
        let state = &mut *self.0.borrow_mut();
        let item_count = state.items.summary().count;
        if scroll_top.item_ix >= item_count {
            scroll_top.item_ix = item_count;
            scroll_top.offset_in_item = px(0.);
        }

        state.scroll_position = ListScrollPosition::Content(scroll_top);
    }

    /// Scroll the list to the given item, such that the item is fully visible.
    pub fn scroll_to_reveal_item(&self, ix: usize) {
        let state = &mut *self.0.borrow_mut();

        let mut scroll_top = state.logical_scroll_top();
        let height = state
            .last_layout_bounds
            .map_or(px(0.), |bounds| bounds.size.height);
        let padding = state.last_padding.unwrap_or_default();

        if ix <= scroll_top.item_ix {
            scroll_top.item_ix = ix;
            scroll_top.offset_in_item = px(0.);
        } else {
            let mut cursor = state.items.cursor::<ListItemSummary>(());
            cursor.seek(&Count(ix + 1), Bias::Right);
            let bottom = cursor.start().height + padding.top;
            let goal_top = px(0.).max(bottom - height + padding.bottom);

            cursor.seek(&Height(goal_top), Bias::Left);
            let start_ix = cursor.start().count;
            let start_item_top = cursor.start().height;

            if start_ix >= scroll_top.item_ix {
                scroll_top.item_ix = start_ix;
                scroll_top.offset_in_item = goal_top - start_item_top;
            }
        }

        state.scroll_position = ListScrollPosition::Content(scroll_top);
    }

    /// Scroll the list so the bottom edge of the given item is visible at the bottom of the viewport.
    pub fn scroll_to_reveal_item_end(&self, ix: usize) {
        let state = &mut *self.0.borrow_mut();

        let height = state
            .last_layout_bounds
            .map_or(px(0.), |bounds| bounds.size.height);
        let padding = state.last_padding.unwrap_or_default();

        let mut cursor = state.items.cursor::<ListItemSummary>(());
        cursor.seek(&Count(ix + 1), Bias::Right);
        let bottom = cursor.start().height + padding.top;
        let goal_top = px(0.).max(bottom - height + padding.bottom);

        cursor.seek(&Height(goal_top), Bias::Left);
        let start_ix = cursor.start().count;
        let start_item_top = cursor.start().height;

        state.scroll_position = ListScrollPosition::Content(ListOffset {
            item_ix: start_ix,
            offset_in_item: goal_top - start_item_top,
        });
    }

    /// Get the bounds for the given item in window coordinates, if it's
    /// been rendered.
    pub fn bounds_for_item(&self, ix: usize) -> Option<Bounds<Pixels>> {
        let state = &*self.0.borrow();

        let bounds = state.last_layout_bounds.unwrap_or_default();
        let scroll_top = state.logical_scroll_top();
        if ix < scroll_top.item_ix {
            return None;
        }

        let mut cursor = state.items.cursor::<Dimensions<Count, Height>>(());
        cursor.seek(&Count(scroll_top.item_ix), Bias::Right);

        let scroll_top = cursor.start().1.0 + scroll_top.offset_in_item;

        cursor.seek_forward(&Count(ix), Bias::Right);
        if let Some(&ListItem::Measured { size, .. } | &ListItem::DirtyMeasured { size, .. }) =
            cursor.item()
        {
            let &Dimensions(Count(count), Height(top), _) = cursor.start();
            if count == ix {
                let top = bounds.top() + top - scroll_top;
                return Some(Bounds::from_corners(
                    point(bounds.left(), top),
                    point(bounds.right(), top + size.height),
                ));
            }
        }
        None
    }

    /// Get the measured size of the given item, even if it is not currently visible.
    pub fn measured_item_size(&self, ix: usize) -> Option<Size<Pixels>> {
        let state = &*self.0.borrow();
        let mut cursor = state.items.cursor::<Dimensions<Count, Height>>(());
        cursor.seek(&Count(ix), Bias::Right);

        if let Some(&ListItem::Measured { size, .. } | &ListItem::DirtyMeasured { size, .. }) =
            cursor.item()
        {
            let &Dimensions(Count(count), _, _) = cursor.start();
            if count == ix {
                return Some(size);
            }
        }

        None
    }

    /// Call this method when the user starts dragging the scrollbar.
    ///
    /// This will prevent the height reported to the scrollbar from changing during the drag
    /// as items in the overdraw get measured, and help offset scroll position changes accordingly.
    pub fn scrollbar_drag_started(&self) {
        let mut state = self.0.borrow_mut();
        state.scrollbar_drag_start_height = Some(state.scrollbar_content_height());
    }

    /// Called when the user stops dragging the scrollbar.
    ///
    /// See `scrollbar_drag_started`.
    pub fn scrollbar_drag_ended(&self) {
        self.0.borrow_mut().scrollbar_drag_start_height.take();
    }

    /// Set the offset from the scrollbar
    pub fn set_offset_from_scrollbar(&self, point: Point<Pixels>) {
        self.0.borrow_mut().set_offset_from_scrollbar(point);
    }

    /// Returns the maximum scroll offset according to the items we have measured.
    /// This value remains constant while dragging to prevent the scrollbar from moving away unexpectedly.
    pub fn max_offset_for_scrollbar(&self) -> Size<Pixels> {
        let state = self.0.borrow();
        let bounds = state.last_layout_bounds.unwrap_or_default();

        let height = state
            .scrollbar_drag_start_height
            .unwrap_or_else(|| state.scrollbar_content_height());

        Size::new(Pixels::ZERO, Pixels::ZERO.max(height - bounds.size.height))
    }

    /// Returns the current scroll offset adjusted for the scrollbar
    pub fn scroll_px_offset_for_scrollbar(&self) -> Point<Pixels> {
        let state = &self.0.borrow();
        let bounds = state.last_layout_bounds.unwrap_or_default();
        let padding = state.last_padding.unwrap_or_default();
        let scroll_top = state.scroll_top_for_position(bounds.size.height, &padding);
        let content_height = state.scrollbar_content_height();
        let drag_offset =
            // if dragging the scrollbar, we want to offset the point if the height changed
            content_height - state.scrollbar_drag_start_height.unwrap_or(content_height);
        let offset = scroll_top - drag_offset;

        Point::new(px(0.), -offset)
    }

    /// Return the bounds of the viewport in pixels.
    pub fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.0.borrow().last_layout_bounds.unwrap_or_default()
    }
}
