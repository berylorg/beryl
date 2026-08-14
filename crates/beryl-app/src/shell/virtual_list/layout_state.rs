use super::*;

impl StateInner {
    pub(super) fn layout_all_items(
        &mut self,
        available_width: Pixels,
        render_item: &mut RenderItemFn,
        window: &mut Window,
        cx: &mut App,
    ) {
        match &mut self.measuring_behavior {
            ListMeasuringBehavior::Visible => {
                return;
            }
            ListMeasuringBehavior::Measure(has_measured) => {
                if *has_measured {
                    return;
                }
                *has_measured = true;
            }
        }

        let cursor = self.items.cursor::<Count>(());
        let available_item_space = size(
            AvailableSpace::Definite(available_width),
            AvailableSpace::MinContent,
        );

        let mut measured_items = Vec::default();

        for (ix, item) in cursor.enumerate() {
            let size = if item.needs_measurement() {
                let mut element = render_item(ix, window, cx);
                element.layout_as_root(available_item_space, window, cx)
            } else {
                item.size().unwrap()
            };

            measured_items.push(ListItem::Measured {
                size,
                focus_handle: item.focus_handle(),
            });
        }

        self.items = SumTree::from_iter(measured_items, ());
    }

    pub(super) fn layout_items(
        &mut self,
        available_width: Option<Pixels>,
        available_height: Pixels,
        padding: &Edges<Pixels>,
        render_item: &mut RenderItemFn,
        window: &mut Window,
        cx: &mut App,
    ) -> LayoutItemsResponse {
        let old_items = self.items.clone();
        let mut measured_items = VecDeque::new();
        let mut item_layouts = VecDeque::new();
        let mut rendered_height = padding.top;
        let mut max_item_width = px(0.);
        let mut scroll_top = self.logical_scroll_top();
        let content_anchor = match self.scroll_position() {
            ListScrollPosition::Content(anchor) => Some(anchor),
            ListScrollPosition::Bottom | ListScrollPosition::VirtualTail { .. } => None,
        };
        let mut content_anchor_height_change = None;
        let mut scroll_position_changed_during_layout = false;
        let mut rendered_focused_item = false;
        let virtual_trailing_height =
            self.visible_virtual_trailing_height(available_height, padding, &scroll_top);

        let available_item_space = size(
            available_width.map_or(AvailableSpace::MinContent, |width| {
                AvailableSpace::Definite(width)
            }),
            AvailableSpace::MinContent,
        );

        let mut cursor = old_items.cursor::<Count>(());

        // Render items after the scroll top, including those in the trailing overdraw
        cursor.seek(&Count(scroll_top.item_ix), Bias::Right);
        for (ix, item) in cursor.by_ref().enumerate() {
            let visible_height = rendered_height - scroll_top.offset_in_item;
            if visible_height >= available_height + self.overdraw {
                break;
            }

            // Use the previously cached height and focus handle if available
            let old_size = item.size();
            let needs_measurement = item.needs_measurement();
            let mut size = old_size;

            // If we're within the visible area or the height wasn't cached, render and measure the item's element
            if visible_height < available_height || needs_measurement {
                let item_index = scroll_top.item_ix + ix;
                let mut element = render_item(item_index, window, cx);
                let element_size = element.layout_as_root(available_item_space, window, cx);
                size = Some(element_size);
                if let Some((item_ix, old_height, new_height)) = content_anchor_height_change_for(
                    content_anchor,
                    item_index,
                    old_size,
                    element_size,
                    needs_measurement,
                ) {
                    content_anchor_height_change = Some((item_ix, old_height, new_height));
                    if item_ix == scroll_top.item_ix {
                        scroll_top.offset_in_item =
                            (scroll_top.offset_in_item + new_height - old_height).max(px(0.0));
                    }
                }
                if visible_height < available_height {
                    item_layouts.push_back(ItemLayout {
                        index: item_index,
                        element,
                        size: element_size,
                    });
                    if item.contains_focused(window, cx) {
                        rendered_focused_item = true;
                    }
                }
            }

            let size = size.unwrap();
            rendered_height += size.height;
            max_item_width = max_item_width.max(size.width);
            measured_items.push_back(ListItem::Measured {
                size,
                focus_handle: item.focus_handle(),
            });
        }
        rendered_height += padding.bottom + virtual_trailing_height;

        // Prepare to start walking upward from the item at the scroll top.
        cursor.seek(&Count(scroll_top.item_ix), Bias::Right);

        // If the rendered items do not fill the visible region, then adjust
        // the scroll top upward.
        if rendered_height - scroll_top.offset_in_item < available_height {
            while rendered_height < available_height {
                cursor.prev();
                if let Some(item) = cursor.item() {
                    let item_index = cursor.start().0;
                    let mut element = render_item(item_index, window, cx);
                    let element_size = element.layout_as_root(available_item_space, window, cx);
                    let focus_handle = item.focus_handle();
                    rendered_height += element_size.height;
                    measured_items.push_front(ListItem::Measured {
                        size: element_size,
                        focus_handle,
                    });
                    item_layouts.push_front(ItemLayout {
                        index: item_index,
                        element,
                        size: element_size,
                    });
                    if item.contains_focused(window, cx) {
                        rendered_focused_item = true;
                    }
                } else {
                    break;
                }
            }

            scroll_top = ListOffset {
                item_ix: cursor.start().0,
                offset_in_item: rendered_height - available_height,
            };

            match (self.alignment, self.scroll_position()) {
                (ListAlignment::Bottom, ListScrollPosition::Bottom) => {
                    scroll_top = ListOffset {
                        item_ix: cursor.start().0,
                        offset_in_item: rendered_height - available_height,
                    };
                }
                _ => {
                    scroll_top.offset_in_item = scroll_top.offset_in_item.max(px(0.));
                    self.scroll_position = ListScrollPosition::Content(scroll_top);
                    scroll_position_changed_during_layout = true;
                }
            };
        }

        // Measure items in the leading overdraw
        let mut leading_overdraw = scroll_top.offset_in_item;
        while leading_overdraw < self.overdraw {
            cursor.prev();
            if let Some(item) = cursor.item() {
                let size = if item.needs_measurement() {
                    let item_index = cursor.start().0;
                    let old_size = item.size();
                    let mut element = render_item(item_index, window, cx);
                    let element_size = element.layout_as_root(available_item_space, window, cx);
                    if let Some(change) = content_anchor_height_change_for(
                        content_anchor,
                        item_index,
                        old_size,
                        element_size,
                        true,
                    ) {
                        content_anchor_height_change = Some(change);
                    }
                    element_size
                } else if let Some(size) = item.size() {
                    size
                } else {
                    let mut element = render_item(cursor.start().0, window, cx);
                    element.layout_as_root(available_item_space, window, cx)
                };

                leading_overdraw += size.height;
                measured_items.push_front(ListItem::Measured {
                    size,
                    focus_handle: item.focus_handle(),
                });
            } else {
                break;
            }
        }

        let measured_range = cursor.start().0..(cursor.start().0 + measured_items.len());
        let mut cursor = old_items.cursor::<Count>(());
        let mut new_items = cursor.slice(&Count(measured_range.start), Bias::Right);
        new_items.extend(measured_items, ());
        cursor.seek(&Count(measured_range.end), Bias::Right);
        new_items.append(cursor.suffix(), ());
        self.items = new_items;

        if !scroll_position_changed_during_layout
            && let Some((item_ix, old_height, new_height)) = content_anchor_height_change
            && let Some(adjusted_scroll_top) =
                self.adjust_content_scroll_for_item_height_change(item_ix, old_height, new_height)
        {
            scroll_top = adjusted_scroll_top;
        }

        // If none of the visible items are focused, check if an off-screen item is focused
        // and include it to be rendered after the visible items so keyboard interaction continues
        // to work for it.
        if !rendered_focused_item {
            let mut cursor = self
                .items
                .filter::<_, Count>((), |summary| summary.has_focus_handles);
            cursor.next();
            while let Some(item) = cursor.item() {
                if item.contains_focused(window, cx) {
                    let item_index = cursor.start().0;
                    let mut element = render_item(cursor.start().0, window, cx);
                    let size = element.layout_as_root(available_item_space, window, cx);
                    item_layouts.push_back(ItemLayout {
                        index: item_index,
                        element,
                        size,
                    });
                    break;
                }
                cursor.next();
            }
        }

        LayoutItemsResponse {
            max_item_width,
            scroll_top,
            item_layouts,
        }
    }

    pub(super) fn prepaint_items(
        &mut self,
        bounds: Bounds<Pixels>,
        padding: Edges<Pixels>,
        autoscroll: bool,
        render_item: &mut RenderItemFn,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<LayoutItemsResponse, ListOffset> {
        window.transact(|window| {
            match self.measuring_behavior {
                ListMeasuringBehavior::Measure(has_measured) if !has_measured => {
                    self.layout_all_items(bounds.size.width, render_item, window, cx);
                }
                _ => {}
            }

            let mut layout_response = self.layout_items(
                Some(bounds.size.width),
                bounds.size.height,
                &padding,
                render_item,
                window,
                cx,
            );

            // Avoid honoring autoscroll requests from elements other than our children.
            window.take_autoscroll();

            // Only paint the visible items, if there is actually any space for them (taking padding into account)
            if bounds.size.height > padding.top + padding.bottom {
                let mut item_origin = bounds.origin + Point::new(px(0.), padding.top);
                item_origin.y -= layout_response.scroll_top.offset_in_item;
                for item in &mut layout_response.item_layouts {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        item.element.prepaint_at(item_origin, window, cx);
                    });

                    if let Some(autoscroll_bounds) = window.take_autoscroll()
                        && autoscroll
                    {
                        if autoscroll_bounds.top() < bounds.top() {
                            return Err(ListOffset {
                                item_ix: item.index,
                                offset_in_item: autoscroll_bounds.top() - item_origin.y,
                            });
                        } else if autoscroll_bounds.bottom() > bounds.bottom() {
                            let mut cursor = self.items.cursor::<Count>(());
                            cursor.seek(&Count(item.index), Bias::Right);
                            let mut height = bounds.size.height - padding.top - padding.bottom;

                            // Account for the height of the element down until the autoscroll bottom.
                            height -= autoscroll_bounds.bottom() - item_origin.y;

                            // Keep decreasing the scroll top until we fill all the available space.
                            while height > Pixels::ZERO {
                                cursor.prev();
                                let Some(item) = cursor.item() else { break };

                                let size = item.size().unwrap_or_else(|| {
                                    let mut item = render_item(cursor.start().0, window, cx);
                                    let item_available_size =
                                        size(bounds.size.width.into(), AvailableSpace::MinContent);
                                    item.layout_as_root(item_available_size, window, cx)
                                });
                                height -= size.height;
                            }

                            return Err(ListOffset {
                                item_ix: cursor.start().0,
                                offset_in_item: if height < Pixels::ZERO {
                                    -height
                                } else {
                                    Pixels::ZERO
                                },
                            });
                        }
                    }

                    item_origin.y += item.size.height;
                }
            } else {
                layout_response.item_layouts.clear();
            }

            Ok(layout_response)
        })
    }

    // Scrollbar support

    pub(super) fn set_offset_from_scrollbar(&mut self, point: Point<Pixels>) {
        let Some(bounds) = self.last_layout_bounds else {
            return;
        };
        let height = bounds.size.height;

        let padding = self.last_padding.unwrap_or_default();
        let content_height = self.scrollbar_content_height();
        let scroll_max = self.effective_scroll_max(height, &padding);
        let drag_offset =
            // if dragging the scrollbar, we want to offset the point if the height changed
            content_height - self.scrollbar_drag_start_height.unwrap_or(content_height);
        let new_scroll_top = (point.y - drag_offset).abs().max(px(0.)).min(scroll_max);
        self.set_scroll_position_from_scroll_top(new_scroll_top, height, &padding);
    }

    pub(super) fn adjust_content_scroll_for_item_height_change(
        &mut self,
        item_ix: usize,
        old_height: Pixels,
        new_height: Pixels,
    ) -> Option<ListOffset> {
        let delta = new_height - old_height;
        if delta == px(0.0) {
            return None;
        }
        let ListScrollPosition::Content(anchor) = self.scroll_position() else {
            return None;
        };
        if anchor.item_ix != item_ix || anchor.offset_in_item <= px(0.0) {
            return None;
        }

        self.scroll_position = ListScrollPosition::Content(ListOffset {
            item_ix,
            offset_in_item: (anchor.offset_in_item + delta).max(px(0.0)),
        });
        let normalized = self.logical_scroll_top();
        self.scroll_position = ListScrollPosition::Content(normalized);
        Some(normalized)
    }
}

fn content_anchor_height_change_for(
    content_anchor: Option<ListOffset>,
    item_index: usize,
    old_size: Option<Size<Pixels>>,
    new_size: Size<Pixels>,
    preserve_following_content: bool,
) -> Option<(usize, Pixels, Pixels)> {
    if !preserve_following_content {
        return None;
    }
    let Some(anchor) = content_anchor else {
        return None;
    };
    if anchor.item_ix != item_index || anchor.offset_in_item <= px(0.0) {
        return None;
    }
    let Some(old_size) = old_size else {
        return None;
    };
    if old_size.height == new_size.height {
        return None;
    }
    Some((item_index, old_size.height, new_size.height))
}
