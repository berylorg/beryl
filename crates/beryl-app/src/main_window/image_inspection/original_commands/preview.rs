use super::OriginalImageCommands;
use crate::{
    image_processing::{ImageMediaWindow, ImageSourceDemand, OriginalImageRequest},
    widgets::anchored_context_menu::{
        self as widget, AnchoredContextMenu, MenuCollection, MenuColors, MenuEvent, MenuIntent,
        MenuRow, MenuRowKind,
    },
};
use gpui::{AnyElement, Bounds, Context, FocusHandle, Pixels, Window};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Weak},
};

pub(crate) struct PreviewImageCommands {
    pub(crate) original: OriginalImageCommands,
    pub(crate) anchor_focus: FocusHandle,
    close: FocusHandle,
    menu: Option<AnchoredContextMenu>,
    pub(crate) anchor_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    serial: u64,
    delegated: Rc<Cell<bool>>,
    live: Rc<Cell<bool>>,
    _guards: [gpui::Subscription; 2],
}
impl PreviewImageCommands {
    pub(crate) fn new<T: 'static>(
        media: Weak<ImageMediaWindow>,
        demand: Option<ImageSourceDemand>,
        close: FocusHandle,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Self {
        let anchor_focus = cx.focus_handle().tab_stop(true);
        let delegated = Rc::new(Cell::new(false));
        let live = Rc::new(Cell::new(true));
        let guard = |focus: &FocusHandle, cx: &mut Context<T>, window: &mut Window| {
            let close = close.clone();
            let anchor = anchor_focus.clone();
            let delegated = delegated.clone();
            let live = live.clone();
            cx.on_blur(focus, window, move |_, window, _| {
                if live.get()
                    && !delegated.get()
                    && !close.is_focused(window)
                    && !anchor.is_focused(window)
                {
                    window.focus(&close);
                }
            })
        };
        let guards = [guard(&close, cx, window), guard(&anchor_focus, cx, window)];
        Self {
            original: OriginalImageCommands::new(media, demand),
            anchor_focus,
            close,
            menu: None,
            anchor_bounds: Rc::new(Cell::new(None)),
            serial: 0,
            delegated,
            live,
            _guards: guards,
        }
    }
    pub(crate) fn poll<T: 'static>(&mut self, window: &mut Window, cx: &mut Context<T>) {
        let picker = self.original.picker_pending();
        self.original.poll(cx);
        self.delegated
            .set(self.menu.is_some() || self.original.picker_pending());
        if picker && !self.original.picker_pending() {
            window.focus(&self.anchor_focus);
        }
    }
    pub(crate) fn focus_delegated(&self, window: &Window) -> bool {
        self.delegated.get() || self.anchor_focus.is_focused(window)
    }
    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_some()
    }
    pub(crate) fn open<T: 'static>(&mut self, window: &mut Window, cx: &mut Context<T>) {
        if self.original.picker_pending() {
            return;
        }
        if self.menu.is_some() {
            self.dismiss(window);
            return;
        }
        let Some(serial) = self.serial.checked_add(1) else {
            return;
        };
        if self.anchor_bounds.get().is_none() {
            return;
        }
        self.serial = serial;
        self.delegated.set(true);
        window.focus(&self.anchor_focus);
        let mut menu = AnchoredContextMenu::open(serial, window, cx);
        menu.reconcile(&self.rows());
        self.menu = Some(menu);
        cx.notify();
    }
    pub(crate) fn dismiss(&mut self, window: &mut Window) {
        if let Some(mut menu) = self.menu.take() {
            menu.dismiss(window, &self.anchor_focus);
        }
        self.delegated.set(self.original.picker_pending());
    }
    pub(crate) fn key(&mut self, key: &str, window: &mut Window) -> bool {
        if key == "tab" && self.menu.is_none() && !self.original.picker_pending() {
            window.focus(if self.anchor_focus.is_focused(window) {
                &self.close
            } else {
                &self.anchor_focus
            });
            true
        } else {
            false
        }
    }
    pub(crate) fn event<T: 'static>(
        &mut self,
        event: MenuEvent,
        window: &mut Window,
        cx: &mut Context<T>,
    ) {
        let rows = self.rows();
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        for intent in menu.input(event, &rows) {
            match intent {
                MenuIntent::Dismiss => self.dismiss(window),
                MenuIntent::Focus(_) => {}
                MenuIntent::Activate { index, .. } => {
                    self.dismiss(window);
                    let command = if index == 0 {
                        OriginalImageRequest::Copy
                    } else {
                        OriginalImageRequest::Save
                    };
                    self.original.start(command, cx);
                    self.delegated.set(self.original.picker_pending());
                }
            }
        }
        cx.notify();
    }
    pub(crate) fn render<T: 'static>(
        &mut self,
        colors: MenuColors,
        selector: &'static str,
        effect: fn(&mut T, MenuEvent, &mut Window, &mut Context<T>),
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Option<AnyElement> {
        let rows = self.rows();
        let menu = self.menu.as_mut()?;
        menu.reconcile(&rows);
        let rows = vec![rows.row(0), rows.row(1)];
        menu.realize(&rows, 0..2, window);
        Some(widget::render_static(
            menu,
            self.anchor_bounds.get()?,
            rows,
            colors,
            selector,
            effect,
            window,
            cx,
        ))
    }
    fn rows(&self) -> Rows {
        Rows {
            copy: self.original.reason(OriginalImageRequest::Copy),
            save: self.original.reason(OriginalImageRequest::Save),
        }
    }
}
impl Drop for PreviewImageCommands {
    fn drop(&mut self) {
        self.live.set(false);
    }
}
struct Rows {
    copy: Option<&'static str>,
    save: Option<&'static str>,
}
impl MenuCollection for Rows {
    fn len(&self) -> usize {
        2
    }
    fn index_of(&self, id: &str) -> Option<usize> {
        match id {
            "image-original-copy" => Some(0),
            "image-original-save" => Some(1),
            _ => None,
        }
    }
    fn row(&self, index: usize) -> MenuRow {
        let (id, label, reason) = if index == 0 {
            ("image-original-copy", "Copy", self.copy)
        } else {
            ("image-original-save", "Save…", self.save)
        };
        MenuRow {
            index,
            id: Some(Arc::from(id)),
            label: label.into(),
            kind: MenuRowKind::Command,
            selected: false,
            enabled: reason.is_none(),
            disabled_reason: reason.map(Into::into),
            selector: id.into(),
        }
    }
}

pub(crate) fn composer_colors() -> MenuColors {
    let background = gpui::rgba(0x20242bff);
    let foreground = gpui::rgba(0xf2f4f7ff);
    let highlight = gpui::rgba(0x333a45ff);
    MenuColors {
        background,
        foreground,
        border: gpui::rgba(0x5f6875ff),
        hover: highlight,
        hover_foreground: foreground,
        pressed: highlight,
        pressed_foreground: foreground,
        focused: highlight,
        focused_foreground: foreground,
        selected: highlight,
        selected_foreground: foreground,
        disabled: gpui::rgba(0x94a3b8ff),
        header: foreground,
        checkmark: foreground,
        tooltip_background: background,
        tooltip_foreground: foreground,
    }
}
