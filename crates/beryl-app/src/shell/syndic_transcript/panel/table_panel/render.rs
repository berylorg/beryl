use super::*;

pub(in super::super) struct RenderSource {
    id: u64,
    owner: super::super::table_owner::TableOwner,
    revision: beryl_model::ProjectionRevision,
    root: Option<index::Root>,
}

impl RenderSource {
    fn capture(table: &MountedTablePanel) -> Rc<Self> {
        Self::for_viewport(table.id, &table.viewport)
    }

    pub(in super::super) fn for_viewport(id: u64, viewport: &TableViewport) -> Rc<Self> {
        Rc::new(Self {
            id,
            owner: viewport.owner.clone(),
            revision: viewport.revision.clone(),
            root: viewport.root.clone(),
        })
    }

    pub(in super::super) fn matches(&self, id: u64, viewport: &TableViewport) -> bool {
        id == self.id
            && match (&viewport.owner, &self.owner) {
                (
                    super::super::table_owner::TableOwner::Indexed(current),
                    super::super::table_owner::TableOwner::Indexed(old),
                ) => {
                    current.resource_id == old.resource_id
                        && current.record_id == old.record_id
                        && current.activation_revision == old.activation_revision
                }
                (
                    super::super::table_owner::TableOwner::Inline(current),
                    super::super::table_owner::TableOwner::Inline(old),
                ) => {
                    current.record_id == old.record_id
                        && current.view_id == old.view_id
                        && current.activation_revision == old.activation_revision
                        && current.prepared == old.prepared
                }
                _ => false,
            }
            && viewport.revision == self.revision
            && viewport.root == self.root
    }

    fn current(&self, panel: &SyndicTranscriptPanel) -> bool {
        panel
            .table_panels
            .iter()
            .any(|table| self.matches(table.id, &table.viewport))
    }
}

struct CellTooltip {
    text: SharedString,
    remaining: bool,
    appearance: TableAppearance,
}

impl gpui::Render for CellTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(480.0))
            .max_h(px(240.0))
            .overflow_hidden()
            .p(px(8.0))
            .border_1()
            .border_color(self.appearance.cell_border)
            .bg(self.appearance.cell_background)
            .text_color(self.appearance.cell_foreground)
            .font(self.appearance.font.clone())
            .text_size(px(self.appearance.font_size))
            .child(self.text.clone())
            .when(self.remaining, |tooltip| {
                tooltip.child(" … (source continues)")
            })
    }
}
pub(in super::super) fn shell(appearance: &TableAppearance, row_height: f32) -> gpui::Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .h(px(appearance.header_height()
            + appearance.viewport_height(row_height)
            + row_height
            + 34.0
            + 2.0))
        .flex()
        .flex_col()
        .overflow_hidden()
        .border_1()
        .rounded(px(7.0))
        .bg(appearance.background)
        .text_color(appearance.foreground)
        .border_color(appearance.border)
}
pub(in super::super) fn render(
    table: &MountedTablePanel,
    appearance: &TableAppearance,
    command_appearance: &super::super::code_appearance::CodeAppearance,
    pending: bool,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> AnyElement {
    #[cfg(test)]
    table.fallback_bounds.borrow_mut().clear();
    let id = table.id;
    let source = RenderSource::capture(table);
    let scroll = table.viewport.scroll.borrow();
    let row_height = scroll.row_height;
    let width = scroll.column_width;
    let x = scroll.horizontal;
    let y = scroll.vertical;
    let realized_columns = table.viewport.realized_ranges().1;
    let mut header_row = div()
        .id(SharedString::from(format!("table-header-row-{id}")))
        .relative()
        .h(px(row_height))
        .w_full()
        .overflow_hidden();
    let mut body = div()
        .id(SharedString::from(format!("table-viewport-{id}")))
        .relative()
        .h(px(appearance.viewport_height(row_height)))
        .w_full()
        .overflow_hidden();
    for paint in table.viewport.paint_cells() {
        let cell = paint.id;
        let TableCellId { role, row, column } = cell;
        let fallback = paint.fallback;
        let selected = table
            .viewport
            .selection
            .as_ref()
            .is_some_and(|selection| selection.contains(cell));
        let focused = table.viewport.focus == Some(cell);
        let text = paint.text;
        let mut element = div()
            .id(SharedString::from(format!(
                "table-cell-{id}-{:?}-{role:?}-{row}-{column}",
                source.owner.record_id()
            )))
            .absolute()
            .left(px(column as f32 * width - x))
            .top(px(if role == index::RowRole::Header {
                0.0
            } else {
                row as f32 * row_height - y
            }))
            .w(px(if fallback {
                width * (realized_columns.end - realized_columns.start) as f32
            } else {
                width
            }))
            .h(px(row_height))
            .px(px(8.0))
            .py(px(5.0))
            .border_r_1()
            .border_b_1()
            .border_color(if role == index::RowRole::Header {
                appearance.header_cell_border
            } else {
                appearance.cell_border
            })
            .font(if role == index::RowRole::Header {
                appearance.header_font.clone()
            } else {
                appearance.font.clone()
            })
            .text_size(px(if role == index::RowRole::Header {
                appearance.header_font_size
            } else {
                appearance.font_size
            }))
            .bg(if selected {
                appearance.selected
            } else if role == index::RowRole::Header {
                appearance.header_cell_background
            } else {
                appearance.cell_background
            })
            .text_color(if selected {
                appearance.selected_foreground
            } else if fallback {
                appearance.unavailable
            } else if role == index::RowRole::Header {
                appearance.header_cell_foreground
            } else {
                appearance.cell_foreground
            })
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .when(focused, |cell| {
                let color = appearance.focused;
                cell.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let mut outline =
                                gpui::outline(bounds, color, gpui::BorderStyle::Solid);
                            outline.border_widths = px(2.0).into();
                            window.paint_quad(outline);
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            });
        element = match paint.alignment {
            Some(index::Alignment::Center) => element.text_center(),
            Some(index::Alignment::Right) => element.text_right(),
            _ => element,
        };
        if fallback {
            let copy_source = source.clone();
            let save_source = source.clone();
            let fallback_row = div()
                .size_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child("Source row fallback · ")
                        .child(text),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("table-row-copy-{id}-{row}")))
                        .flex_none()
                        .cursor_pointer()
                        .child("Copy row")
                        .on_click(cx.listener(move |panel, _, _, cx| {
                            if copy_source.current(panel) {
                                panel.begin_table_row_copy(id, row, cx);
                            }
                            cx.stop_propagation();
                        })),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("table-row-save-{id}-{row}")))
                        .flex_none()
                        .cursor_pointer()
                        .child("Save row…")
                        .on_click(cx.listener(move |panel, _, window, cx| {
                            if save_source.current(panel) {
                                panel.begin_table_row_save(id, row, window, cx);
                            }
                            cx.stop_propagation();
                        })),
                );
            #[cfg(test)]
            let fallback_row = {
                let measured = table.fallback_bounds.clone();
                fallback_row.on_children_prepainted(move |bounds, _, _| {
                    if let Some((copy, save)) = bounds.get(1).copied().zip(bounds.get(2).copied()) {
                        let mut measured = measured.borrow_mut();
                        if measured.len() < 15 {
                            measured.push((row, copy, save));
                        }
                    }
                })
            };
            element = element.child(fallback_row);
        } else {
            let pointer_source = source.clone();
            if paint.tooltip {
                let tooltip_text = text.clone();
                let remaining = paint.remaining;
                let appearance = appearance.clone();
                element = element.tooltip(move |_, cx| {
                    cx.new(|_| CellTooltip {
                        text: tooltip_text.clone(),
                        remaining,
                        appearance: appearance.clone(),
                    })
                    .into()
                });
            }
            element = element.child(text).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |panel, event: &gpui::MouseDownEvent, window, cx| {
                    if !pointer_source.current(panel) {
                        cx.stop_propagation();
                        return;
                    }
                    if let Some(table) = panel.table_panels.iter_mut().find(|table| table.id == id)
                    {
                        window.focus(&table.focus);
                        table.viewport.select(cell, event.modifiers.shift);
                        table.viewport.dragging = true;
                        panel.table_wheel_owner.set(Some(id));
                        panel.code_wheel_owner.set(None);
                        panel.keyboard_copy.gesture.reset();
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        }
        if role == index::RowRole::Header {
            header_row = header_row.child(element);
        } else {
            body = body.child(element);
        }
    }
    drop(scroll);
    if table.viewport.source_pages().is_empty() && table.viewport.owner.inline().is_none() {
        body = body.child(
            div()
                .text_color(appearance.pending)
                .child("Table range loading"),
        );
    }
    let measured = table.viewport.scroll.clone();
    let header_measured = measured.clone();
    let visibility = table.visibility.clone();
    let header_source = source.clone();
    header_row = header_row
        .child(
            canvas(
                move |bounds, _, _| {
                    header_measured.borrow_mut().header_bounds = bounds;
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .on_mouse_move(
            cx.listener(move |panel, event: &gpui::MouseMoveEvent, _, cx| {
                if header_source.current(panel) {
                    panel.table_drag(id, event, cx)
                }
            }),
        );
    body = body.child(
        canvas(
            move |bounds, _, _| {
                measured.borrow_mut().bounds = bounds;
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full(),
    );
    let drag_source = source.clone();
    let pointer_source = source.clone();
    let keyboard_source = source.clone();
    body = body
        .track_focus(&table.focus)
        .tab_stop(true)
        .on_mouse_move(
            cx.listener(move |panel, event: &gpui::MouseMoveEvent, _, cx| {
                if drag_source.current(panel) {
                    panel.table_drag(id, event, cx)
                }
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |panel, _, _, _| {
                if let Some(table) = panel.table_panels.iter_mut().find(|table| table.id == id) {
                    table.viewport.dragging = false;
                }
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |panel, _, _, _| {
                if let Some(table) = panel.table_panels.iter_mut().find(|table| table.id == id) {
                    table.viewport.dragging = false;
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |panel, event: &gpui::MouseDownEvent, window, cx| {
                if !pointer_source.current(panel) {
                    cx.stop_propagation();
                    return;
                }
                if let Some(table) = panel.table_panels.iter_mut().find(|table| table.id == id) {
                    window.focus(&table.focus);
                    if let Some(cell) = table.viewport.hit(event.position) {
                        table.viewport.request_cell(cell, event.modifiers.shift);
                        table.viewport.dragging = !table.viewport.fallback_row(cell);
                        panel.keyboard_copy.gesture.reset();
                    }
                    panel.table_wheel_owner.set(Some(id));
                    panel.code_wheel_owner.set(None);
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_key_down(
            cx.listener(move |panel, event: &gpui::KeyDownEvent, window, cx| {
                if !keyboard_source.current(panel) {
                    cx.stop_propagation();
                    return;
                }
                if super::super::copy_keyboard::is_copy(event) {
                    panel.copy_table_key(id, event, window, cx);
                    cx.stop_propagation();
                } else {
                    panel.table_navigate(id, event, cx);
                }
            }),
        );
    for axis in [Axis::Vertical, Axis::Horizontal] {
        if let Some(bar) = render_scrollbar(
            SharedString::from(format!("table-scrollbar-{id}-{axis:?}")),
            table.scrollbar.clone(),
            axis,
            command_appearance.scrollbar,
            visibility.clone(),
            table.interaction.clone(),
        ) {
            body = body.child(bar);
        }
    }
    let count = table.viewport.counts().map_or_else(
        || "Table loading".to_owned(),
        |(rows, columns)| format!("Table · {rows} rows · {columns} columns"),
    );
    let header = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(appearance.header_height()))
        .px(px(10.0))
        .bg(appearance.header)
        .text_color(appearance.header_foreground)
        .border_b_1()
        .border_color(appearance.header_border)
        .child(div().flex_1().child(count))
        .child(command(table, false, pending, command_appearance, cx))
        .child(command(table, true, pending, command_appearance, cx));
    #[cfg(test)]
    let header = {
        let measured = table.command_bounds.clone();
        header.on_children_prepainted(move |bounds, _, _| {
            measured.set(bounds.get(1).copied().zip(bounds.get(2).copied()));
        })
    };
    shell(appearance, row_height)
        .id(SharedString::from(format!("table-panel-{id}")))
        .on_scroll_wheel(
            cx.listener(move |panel, event: &gpui::ScrollWheelEvent, window, cx| {
                panel.table_wheel(id, event, window, cx)
            }),
        )
        .child(header)
        .child(header_row)
        .child(body)
        .child(
            div()
                .h(px(34.0))
                .px(px(8.0))
                .overflow_hidden()
                .text_size(px(11.0))
                .child(if table.actions.feedback.is_empty() {
                    table.viewport.feedback
                } else {
                    table.actions.feedback
                }),
        )
        .into_any_element()
}
fn command(
    table: &MountedTablePanel,
    saving: bool,
    pending: bool,
    appearance: &super::super::code_appearance::CodeAppearance,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> AnyElement {
    let id = table.id;
    let source = RenderSource::capture(table);
    let pointer_source = source.clone();
    let click_source = source.clone();
    let enabled = !pending
        && (table.viewport.binding.is_some() || table.viewport.owner.inline().is_some())
        && !table.actions.pending();
    let focus = if saving {
        table.save_focus.clone()
    } else {
        table.copy_focus.clone()
    };
    let pointer = focus.clone();
    let theme = appearance.command_theme;
    let state = if enabled {
        theme.normal
    } else {
        theme.disabled
    };
    div()
        .id(SharedString::from(format!(
            "table-{}-{id}",
            if saving { "save" } else { "copy" }
        )))
        .track_focus(&focus)
        .tab_stop(true)
        .h(px(30.0))
        .px(px(8.0))
        .border_1()
        .rounded(px(4.0))
        .border_color(state.border)
        .bg(state.background)
        .text_color(state.foreground)
        .font(appearance.command_font.clone())
        .text_size(px(13.0))
        .child(if saving { "Save…" } else { "Copy" })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |panel, _, window, cx| {
                if enabled && pointer_source.current(panel) {
                    window.focus(&pointer);
                }
                cx.stop_propagation();
            }),
        )
        .on_click(cx.listener(move |panel, _, window, cx| {
            if enabled && click_source.current(panel) {
                if saving {
                    panel.begin_table_save(id, window, cx);
                } else {
                    panel.begin_table_copy(
                        id,
                        super::super::copy_keyboard::CopyMethod::Markdown,
                        cx,
                    );
                }
            }
            cx.stop_propagation();
        }))
        .on_key_down(
            cx.listener(move |panel, event: &gpui::KeyDownEvent, window, cx| {
                if !source.current(panel) {
                    cx.stop_propagation();
                    return;
                }
                if super::super::copy_keyboard::is_copy(event) {
                    panel.copy_table_key(id, event, window, cx);
                    cx.stop_propagation();
                } else if enabled
                    && !event.is_held
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                {
                    if saving {
                        panel.begin_table_save(id, window, cx);
                    } else {
                        panel.begin_table_copy(
                            id,
                            super::super::copy_keyboard::CopyMethod::Markdown,
                            cx,
                        );
                    }
                    cx.stop_propagation();
                }
            }),
        )
        .into_any_element()
}
