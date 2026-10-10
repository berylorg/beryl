use std::{ops::Range, sync::Arc};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AnyView, AppContext, Bounds, Context, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled,
    UniformListScrollHandle, WeakFocusHandle, Window, div, point, px, size,
};

pub(crate) const ROW_HEIGHT: f32 = 30.;
pub(crate) const OVERSCAN: usize = 0;

#[derive(Clone)]
pub(crate) struct MenuRow {
    pub(crate) index: usize,
    pub(crate) id: Option<Arc<str>>,
    pub(crate) label: SharedString,
    pub(crate) kind: MenuRowKind,
    pub(crate) selected: bool,
    pub(crate) enabled: bool,
    pub(crate) disabled_reason: Option<SharedString>,
    pub(crate) selector: SharedString,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuRowKind {
    Selection,
    Command,
    Header,
    PendingSelection,
}

pub(crate) trait MenuCollection {
    fn len(&self) -> usize;
    fn row(&self, index: usize) -> MenuRow;
    fn index_of(&self, id: &str) -> Option<usize>;
}

#[derive(Clone)]
pub(crate) enum MenuEvent {
    Key {
        invocation: u64,
        key: String,
        held: bool,
    },
    Pointer {
        invocation: u64,
        index: usize,
        id: Arc<str>,
    },
    Dismiss {
        invocation: u64,
    },
}

pub(crate) enum MenuIntent {
    Focus(usize),
    Activate { index: usize, id: Arc<str> },
    Dismiss,
}

pub(crate) struct AnchoredContextMenu {
    invocation: u64,
    focus: FocusHandle,
    return_focus: Option<WeakFocusHandle>,
    pub(crate) scroll: UniformListScrollHandle,
    focused: Option<(usize, Option<Arc<str>>)>,
    realized_ids: Vec<(usize, Arc<str>)>,
    pending_activation: Option<(usize, Option<Arc<str>>)>,
    content_width: Pixels,
    pub(crate) range: Range<usize>,
    pub(crate) realized: usize,
    pub(crate) reconciliation_micros: u64,
}

impl AnchoredContextMenu {
    pub(crate) fn open<E: 'static>(
        invocation: u64,
        window: &mut Window,
        cx: &mut Context<E>,
    ) -> Self {
        let focus = cx.focus_handle();
        let return_focus = window.focused(cx).map(|focus| focus.downgrade());
        window.focus(&focus);
        Self {
            invocation,
            focus,
            return_focus,
            scroll: UniformListScrollHandle::new(),
            focused: None,
            realized_ids: Vec::new(),
            pending_activation: None,
            content_width: px(160.),
            range: 0..0,
            realized: 0,
            reconciliation_micros: 0,
        }
    }

    pub(crate) fn dismiss(&mut self, window: &mut Window, fallback: &FocusHandle) {
        if self.focus.is_focused(window) {
            if let Some(focus) = self.return_focus.take().and_then(|focus| focus.upgrade()) {
                window.focus(&focus);
            } else {
                window.focus(fallback);
            }
        }
    }

    pub(crate) fn focused_index(&self) -> Option<usize> {
        self.focused.as_ref().map(|(index, _)| *index)
    }
    pub(crate) fn has_focus(&self) -> bool {
        self.focused.is_some()
    }
    pub(crate) fn focused_id(&self) -> Option<&str> {
        self.focused.as_ref().and_then(|(_, id)| id.as_deref())
    }

    pub(crate) fn reconcile(&mut self, rows: &impl MenuCollection) -> bool {
        let previous = self.focused.clone();
        if rows.len() == 0 {
            self.focused = None;
            self.pending_activation = None;
            return previous.is_some();
        }
        if let Some((index, id)) = &mut self.focused {
            if let Some(stable) = id {
                if let Some(current) = rows.index_of(stable) {
                    *index = current;
                } else if rows.row((*index).min(rows.len().saturating_sub(1))).kind
                    != MenuRowKind::PendingSelection
                {
                    self.focused = None;
                }
            } else if *index < rows.len() {
                let row = rows.row(*index);
                if row.kind == MenuRowKind::Header {
                    self.focused = None;
                } else {
                    *id = row.id;
                }
            } else {
                self.focused = None;
            }
        }
        if self.focused.is_none() {
            self.pending_activation = None;
            for index in 0..rows.len() {
                let row = rows.row(index);
                if row.kind != MenuRowKind::Header {
                    self.focused = Some((index, row.id));
                    break;
                }
            }
        }
        self.focused != previous
    }

    pub(crate) fn focus_row(&mut self, index: usize, rows: &impl MenuCollection, reveal: bool) {
        if index >= rows.len() {
            return;
        }
        let row = rows.row(index);
        if row.kind == MenuRowKind::Header {
            return;
        }
        let focused = Some((index, row.id));
        let same_pending_target =
            self.pending_activation
                .as_ref()
                .is_some_and(|(requested, expected)| match expected {
                    Some(expected) => {
                        focused.as_ref().and_then(|(_, id)| id.as_ref()) == Some(expected)
                    }
                    None => *requested == index,
                });
        if self.focused != focused && !same_pending_target {
            self.pending_activation = None;
        }
        self.focused = focused;
        if reveal {
            self.reveal(index);
        }
    }

    pub(crate) fn reveal(&self, index: usize) {
        self.scroll
            .scroll_to_item(index, gpui::ScrollStrategy::Center);
    }

    pub(crate) fn input(
        &mut self,
        event: MenuEvent,
        rows: &impl MenuCollection,
    ) -> Vec<MenuIntent> {
        let invocation = match &event {
            MenuEvent::Key { invocation, .. }
            | MenuEvent::Pointer { invocation, .. }
            | MenuEvent::Dismiss { invocation } => *invocation,
        };
        if invocation != self.invocation {
            return Vec::new();
        }
        self.reconcile(rows);
        match event {
            MenuEvent::Dismiss { .. } => vec![MenuIntent::Dismiss],
            MenuEvent::Pointer { index, id, .. } => {
                if index >= rows.len() {
                    return Vec::new();
                }
                let row = rows.row(index);
                if row.id.as_deref() != Some(id.as_ref())
                    || !row.enabled
                    || row.kind == MenuRowKind::Header
                    || !self
                        .realized_ids
                        .iter()
                        .any(|(realized, stable)| *realized == index && stable == &id)
                {
                    return Vec::new();
                }
                self.focus_row(index, rows, false);
                vec![MenuIntent::Focus(index), MenuIntent::Activate { index, id }]
            }
            MenuEvent::Key { key, held, .. } => {
                if key == "escape" {
                    return vec![MenuIntent::Dismiss];
                }
                if rows.len() == 0 {
                    return Vec::new();
                }
                if key == "enter" || key == "space" {
                    if held {
                        return Vec::new();
                    }
                    let Some((index, id)) = self.focused.clone() else {
                        return Vec::new();
                    };
                    let row = rows.row(index);
                    if row.kind == MenuRowKind::PendingSelection || (row.enabled && row.id == id) {
                        if let Some(id) = id.as_ref().filter(|id| {
                            self.realized_ids
                                .iter()
                                .any(|(realized, stable)| *realized == index && stable == *id)
                        }) {
                            return vec![MenuIntent::Activate {
                                index,
                                id: id.clone(),
                            }];
                        }
                        self.pending_activation = Some((index, id));
                        self.reveal(index);
                        return vec![MenuIntent::Focus(index)];
                    }
                    return Vec::new();
                }
                let last = rows.len().saturating_sub(1);
                let current = self.focused_index().unwrap_or(0).min(last);
                let (mut next, backwards) = match key.as_str() {
                    "home" => (0, false),
                    "end" => (last, true),
                    "up" => (current.saturating_sub(1), true),
                    "down" => ((current + 1).min(last), false),
                    _ => return Vec::new(),
                };
                loop {
                    if rows.row(next).kind != MenuRowKind::Header {
                        self.focus_row(next, rows, true);
                        return vec![MenuIntent::Focus(next)];
                    }
                    if backwards {
                        if next == 0 {
                            return Vec::new();
                        }
                        next -= 1;
                    } else {
                        if next == last {
                            return Vec::new();
                        }
                        next += 1;
                    }
                }
            }
        }
    }

    pub(crate) fn realize(
        &mut self,
        rows: &[MenuRow],
        range: Range<usize>,
        window: &Window,
    ) -> bool {
        let started = std::time::Instant::now();
        let previous = self.content_width;
        self.range = range;
        self.realized = rows.len();
        self.realized_ids = rows
            .iter()
            .take(12)
            .filter_map(|row| row.id.as_ref().map(|id| (row.index, id.clone())))
            .collect();
        let mut style = window.text_style();
        style.font_weight = gpui::FontWeight::NORMAL;
        for row in rows {
            let width = window
                .text_system()
                .shape_line(
                    row.label.clone(),
                    px(if row.kind == MenuRowKind::Header {
                        12.
                    } else {
                        13.
                    }),
                    &[style.to_run(row.label.len())],
                    None,
                )
                .width
                + px(if row.kind == MenuRowKind::Selection {
                    44.
                } else {
                    20.
                });
            self.content_width = self.content_width.max(width).min(px(480.));
        }
        self.reconciliation_micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.content_width != previous
    }

    pub(crate) fn take_pending_activation(
        &mut self,
        rows: &impl MenuCollection,
    ) -> Option<MenuEvent> {
        let (requested, expected) = self.pending_activation.as_ref()?;
        let (index, Some(id)) = self.focused.as_ref()? else {
            return None;
        };
        if expected.as_ref().is_some_and(|expected| expected != id)
            || (expected.is_none() && requested != index)
        {
            self.pending_activation = None;
            return None;
        }
        if !self
            .realized_ids
            .iter()
            .any(|(realized, stable)| realized == index && stable == id)
        {
            return None;
        }
        let row = rows.row(*index);
        if !row.enabled || row.id.as_ref() != Some(id) {
            self.pending_activation = None;
            return None;
        }
        let event = MenuEvent::Pointer {
            invocation: self.invocation,
            index: *index,
            id: id.clone(),
        };
        self.pending_activation = None;
        Some(event)
    }

    pub(crate) fn bounds(
        &self,
        anchor: Bounds<Pixels>,
        count: usize,
        viewport: gpui::Size<Pixels>,
    ) -> Bounds<Pixels> {
        let width = self
            .content_width
            .max(px(160.))
            .min(px(480.))
            .min((viewport.width - px(16.)).max(px(0.)));
        let height = px(320.)
            .min((viewport.height - px(16.)).max(px(0.)))
            .min(px(ROW_HEIGHT) * count + px(8.));
        let left = anchor
            .left()
            .min((viewport.width - width - px(8.)).max(px(0.)))
            .max(px(0.));
        let above = anchor.top() - px(4.) - height;
        let below = anchor.bottom() + px(4.);
        let top = if above >= px(0.) {
            above
        } else if below + height <= viewport.height {
            below
        } else {
            above
                .max(px(0.))
                .min((viewport.height - height).max(px(0.)))
        };
        Bounds::new(point(left, top), size(width, height))
    }
}

#[derive(Clone, Copy)]
pub(crate) struct MenuColors {
    pub(crate) background: gpui::Rgba,
    pub(crate) foreground: gpui::Rgba,
    pub(crate) border: gpui::Rgba,
    pub(crate) hover: gpui::Rgba,
    pub(crate) hover_foreground: gpui::Rgba,
    pub(crate) pressed: gpui::Rgba,
    pub(crate) pressed_foreground: gpui::Rgba,
    pub(crate) focused: gpui::Rgba,
    pub(crate) focused_foreground: gpui::Rgba,
    pub(crate) selected: gpui::Rgba,
    pub(crate) selected_foreground: gpui::Rgba,
    pub(crate) disabled: gpui::Rgba,
    pub(crate) header: gpui::Rgba,
    pub(crate) checkmark: gpui::Rgba,
    pub(crate) tooltip_background: gpui::Rgba,
    pub(crate) tooltip_foreground: gpui::Rgba,
}

pub(crate) fn render<E: 'static>(
    menu: &AnchoredContextMenu,
    anchor: Bounds<Pixels>,
    count: usize,
    colors: MenuColors,
    selectors: (&'static str, &'static str),
    provider: fn(&mut E, Range<usize>, &mut Window, &mut Context<E>) -> Vec<MenuRow>,
    effect: fn(&mut E, MenuEvent, &mut Window, &mut Context<E>),
    window: &Window,
    cx: &mut Context<E>,
) -> AnyElement {
    let bounds = menu.bounds(anchor, count, window.viewport_size());
    let height = (bounds.size.height - px(8.)).max(px(0.));
    let focused = menu.focused_id().map(Arc::<str>::from);
    let invocation = menu.invocation;
    let weak = cx.weak_entity();
    let list = gpui::uniform_list(selectors.1, count, move |range, window, app| {
        weak.update(app, |owner, cx| {
            let rows = provider(owner, range, window, cx);
            render_rows(rows, focused.as_deref(), colors, invocation, effect, cx)
        })
        .unwrap_or_default()
    })
    .h(height)
    .w_full()
    .track_scroll(menu.scroll.clone());
    let selector = selectors.0;
    div()
        .id(selector)
        .debug_selector(move || selector.to_owned())
        .absolute()
        .left(bounds.origin.x)
        .top(bounds.origin.y)
        .w(bounds.size.width)
        .h(bounds.size.height)
        .overflow_hidden()
        .flex()
        .flex_col()
        .py(px(4.))
        .border_1()
        .border_color(colors.border)
        .rounded(px(6.))
        .shadow(vec![gpui::BoxShadow {
            color: gpui::rgba(0x0f172a2e).into(),
            offset: point(px(0.), px(12.)),
            blur_radius: px(28.),
            spread_radius: px(0.),
        }])
        .bg(colors.background)
        .text_color(colors.foreground)
        .text_size(px(13.))
        .track_focus(&menu.focus)
        .on_key_down(
            cx.listener(move |owner, event: &gpui::KeyDownEvent, window, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "escape" | "home" | "end" | "up" | "down" | "enter" | "space"
                ) {
                    effect(
                        owner,
                        MenuEvent::Key {
                            invocation,
                            key: event.keystroke.key.clone(),
                            held: event.is_held,
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
        )
        .on_mouse_down_out(
            cx.listener(move |owner, event: &gpui::MouseDownEvent, window, cx| {
                if !anchor.contains(&event.position) {
                    effect(owner, MenuEvent::Dismiss { invocation }, window, cx);
                }
            }),
        )
        .when(height > px(0.), |panel| panel.child(list))
        .into_any_element()
}

fn render_rows<E: 'static>(
    rows: Vec<MenuRow>,
    focused: Option<&str>,
    colors: MenuColors,
    invocation: u64,
    effect: fn(&mut E, MenuEvent, &mut Window, &mut Context<E>),
    cx: &mut Context<E>,
) -> Vec<AnyElement> {
    rows.into_iter()
        .map(|row| {
            if matches!(
                row.kind,
                MenuRowKind::Header | MenuRowKind::PendingSelection
            ) {
                return div()
                    .h(px(ROW_HEIGHT))
                    .w_full()
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(12.))
                    .text_color(colors.header)
                    .child(row.label)
                    .into_any_element();
            }
            let id = row.id.expect("interactive menu row has stable identity");
            let index = row.index;
            let selector = row.selector;
            let focus = focused == Some(id.as_ref());
            let mut element = div()
                .id(SharedString::new(id.clone()))
                .debug_selector(move || selector.to_string())
                .h(px(ROW_HEIGHT))
                .w_full()
                .px(px(10.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .gap(px(8.))
                .overflow_hidden()
                .whitespace_nowrap()
                .when(row.selected, |item| {
                    item.bg(colors.selected)
                        .text_color(colors.selected_foreground)
                })
                .when(focus, |item| {
                    item.bg(colors.focused)
                        .text_color(colors.focused_foreground)
                });
            if row.enabled {
                element = element
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover).text_color(colors.hover_foreground))
                    .active(move |style| {
                        style
                            .bg(colors.pressed)
                            .text_color(colors.pressed_foreground)
                    })
                    .on_click(cx.listener(move |owner, _, window, cx| {
                        effect(
                            owner,
                            MenuEvent::Pointer {
                                invocation,
                                index,
                                id: id.clone(),
                            },
                            window,
                            cx,
                        )
                    }));
            } else {
                element = element.text_color(colors.disabled);
                if let Some(reason) = row.disabled_reason {
                    element = element.tooltip(move |_, cx| -> AnyView {
                        cx.new(|_| MenuTooltip(reason.clone(), colors)).into()
                    });
                }
            }
            element
                .when(row.kind == MenuRowKind::Selection, |item| {
                    item.child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .text_color(colors.checkmark)
                            .child(if row.selected { "✓" } else { "" }),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(row.label),
                )
                .into_any_element()
        })
        .collect()
}

struct MenuTooltip(SharedString, MenuColors);
impl Render for MenuTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(320.))
            .max_h(px(160.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .bg(self.1.tooltip_background)
            .text_color(self.1.tooltip_foreground)
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/model_menu_widget.rs"
    ));
}
