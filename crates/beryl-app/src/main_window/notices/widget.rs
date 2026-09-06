mod detail;
mod frame;
mod model;
pub use model::*;
mod render;

use std::{
    fmt,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use beryl_state::{ThemePropertyId as Property, ThemeValue};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement,
    Styled, Window, div, point, px, rgb,
};
use gpui_scrollbar::{
    Axis as ScrollbarAxis, ScrollbarMountGeneration, ScrollbarOwnerId, ScrollbarOwnerKey,
    ScrollbarState, ScrollbarStyle, ScrollbarVisibilityPolicy, render_scroll_handle_scrollbar,
};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    NoticeCommandId, NoticeCommandState, NoticeContent, NoticeDismissal, NoticeProjection,
    NoticeVariant, NoticeVisibleToken,
};
use detail::{DetailSelection, DetailState, NoticeDetailElement};

const NOTICE_WIDTH: f32 = 420.;
const NOTICE_MAX_HEIGHT: f32 = 280.;
const NOTICE_DETAIL_MAX_HEIGHT: f32 = 220.;

static NEXT_WIDGET_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

fn next_widget_instance_id() -> u64 {
    NEXT_WIDGET_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

pub struct MainWindowNoticeWidget {
    widget_instance_id: u64,
    appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
    safe_focus: FocusHandle,
    detail_focus: FocusHandle,
    close_focus: FocusHandle,
    command_focuses: Vec<(NoticeCommandId, FocusHandle)>,
    record: Option<MainWindowNoticeWidgetRecord>,
    on_event: Rc<dyn Fn(MainWindowNoticeWidgetEvent)>,
    detail_state: DetailState,
    dismissal_pending: bool,
    inert: bool,
    paint_phase: MainWindowNoticePaintPhase,
    replacements: u64,
    interaction_generation: u64,
    keyboard_press: Option<(NoticeVisibleToken, Option<NoticeCommandId>, String)>,
    keyboard_blur: Option<gpui::Subscription>,
    frame_size: Rc<std::cell::Cell<gpui::Size<gpui::Pixels>>>,
    scrollbar_state: ScrollbarState,
    scrollbar_owner: ScrollbarOwnerKey,
    scrollbar_mount_generation: u64,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    detail_scroll: ScrollHandle,
}

impl gpui::EventEmitter<MainWindowNoticeWidgetEvent> for MainWindowNoticeWidget {}

impl MainWindowNoticeWidget {
    pub fn set_safe_focus(&mut self, safe_focus: FocusHandle) {
        self.safe_focus = safe_focus;
    }

    pub fn appearance(&self) -> &Arc<crate::theme_runtime::AppearanceGeneration> {
        &self.appearance
    }

    pub fn new(
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
        safe_focus: FocusHandle,
        on_event: impl Fn(MainWindowNoticeWidgetEvent) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        let widget_instance_id = next_widget_instance_id();
        let scrollbar_owner = ScrollbarOwnerKey {
            owner_id: ScrollbarOwnerId::new(widget_instance_id),
            mount_generation: ScrollbarMountGeneration::new(1),
        };
        let scrollbar_state = ScrollbarState::new(scrollbar_owner);
        let scrollbar_visibility =
            scrollbar_state.managed(Rc::new(|_, window, _| window.refresh()));
        Self {
            widget_instance_id,
            appearance,
            safe_focus,
            detail_focus: cx.focus_handle(),
            close_focus: cx.focus_handle(),
            command_focuses: Vec::with_capacity(3),
            record: None,
            on_event: Rc::new(on_event),
            detail_state: DetailState::default(),
            dismissal_pending: false,
            inert: false,
            paint_phase: MainWindowNoticePaintPhase::Visible,
            replacements: 0,
            interaction_generation: 0,
            keyboard_press: None,
            keyboard_blur: None,
            frame_size: Default::default(),
            scrollbar_state,
            scrollbar_owner,
            scrollbar_mount_generation: 1,
            scrollbar_visibility,
            detail_scroll: ScrollHandle::new(),
        }
    }

    pub fn replace(
        &mut self,
        record: Option<MainWindowNoticeWidgetRecord>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let detail_was_focused = self.detail_focus.is_focused(window);
        let close_was_focused = self.close_focus.is_focused(window);
        let focused_command = self
            .command_focuses
            .iter()
            .find(|(_, focus)| focus.is_focused(window))
            .map(|(id, _)| id.clone());
        let previous = self.record.as_ref().map(|value| value.token.clone());
        let replacement = record.as_ref().map(|value| value.token.clone());
        let same_identity = previous
            .as_ref()
            .zip(replacement.as_ref())
            .is_some_and(|(left, right)| left.record().same_identity(right.record()));
        let same_revision = previous
            .as_ref()
            .zip(replacement.as_ref())
            .is_some_and(|(left, right)| left.record().revision() == right.record().revision());
        if !same_identity {
            self.detail_state.clear();
            self.detail_state.retire_layout();
            self.detail_scroll.set_offset(point(px(0.), px(0.)));
        } else {
            let geometry_may_change = !same_revision
                || self
                    .record
                    .as_ref()
                    .zip(record.as_ref())
                    .is_some_and(|(old, new)| old.allocation != new.allocation);
            if !same_revision {
                self.detail_state.clear();
            }
            if geometry_may_change
                && !self
                    .detail_state
                    .capture_scroll_anchor(-self.detail_scroll.offset().y)
            {
                self.detail_scroll.set_offset(point(px(0.), px(0.)));
            }
        }
        if previous != replacement {
            self.interaction_generation = self.interaction_generation.wrapping_add(1);
            self.keyboard_press = None;
            self.keyboard_blur = None;
            self.replacements = self.replacements.saturating_add(1);
        }
        if previous != replacement {
            self.dismissal_pending = false;
        }
        self.record = record;
        if self
            .record
            .as_ref()
            .is_none_or(|record| record.content.detail().as_str().is_empty())
        {
            self.detail_state.retire_layout();
            self.detail_scroll.set_offset(point(px(0.), px(0.)));
        }
        if previous != replacement {
            self.detail_state
                .invalidate_interaction(!self.inert && self.record.is_some());
            let offset = self.detail_scroll.offset();
            self.detail_scroll = ScrollHandle::new();
            self.detail_scroll.set_offset(offset);
        }
        if self.record.is_none() {
            self.frame_size.set(Default::default());
        }
        if previous != replacement {
            self.rotate_scrollbar_owner(window, cx);
        }
        self.reconcile_command_focuses(cx);
        self.reconcile_focus(
            window,
            detail_was_focused,
            close_was_focused,
            focused_command.as_ref(),
        );
        cx.notify();
    }

    pub fn set_inert(&mut self, inert: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.inert == inert {
            return;
        }
        self.inert = inert;
        self.dismissal_pending = false;
        self.interaction_generation = self.interaction_generation.wrapping_add(1);
        self.detail_state
            .invalidate_interaction(!inert && self.record.is_some());
        self.keyboard_press = None;
        self.keyboard_blur = None;
        if inert {
            let offset = self.detail_scroll.offset();
            self.detail_scroll = ScrollHandle::new();
            self.detail_scroll.set_offset(offset);
            self.detail_state.clear();
            self.scrollbar_state
                .unmount_viewport(self.scrollbar_owner, window, cx);
            if matches!(
                self.focus_kind(window),
                MainWindowNoticeControlFocus::Detail
                    | MainWindowNoticeControlFocus::Close
                    | MainWindowNoticeControlFocus::Command
            ) {
                self.safe_focus.focus(window);
            }
        } else if self
            .record
            .as_ref()
            .is_some_and(|record| !record.content.detail().as_str().is_empty())
        {
            self.rotate_scrollbar_owner(window, cx);
        }
        cx.notify();
    }

    pub fn set_appearance(
        &mut self,
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Arc::ptr_eq(&self.appearance, &appearance) {
            return;
        }
        if !self
            .detail_state
            .capture_scroll_anchor(-self.detail_scroll.offset().y)
        {
            self.detail_scroll.set_offset(point(px(0.), px(0.)));
        }
        self.appearance = appearance;
        self.interaction_generation = self.interaction_generation.wrapping_add(1);
        self.keyboard_press = None;
        self.keyboard_blur = None;
        self.detail_state.reset_navigation();
        self.detail_state
            .invalidate_interaction(!self.inert && self.record.is_some());
        let offset = self.detail_scroll.offset();
        self.detail_scroll = ScrollHandle::new();
        self.detail_scroll.set_offset(offset);
        self.rotate_scrollbar_owner(window, cx);
        cx.notify();
    }

    pub fn set_paint_phase(&mut self, phase: MainWindowNoticePaintPhase, cx: &mut Context<Self>) {
        if self.paint_phase != phase {
            self.paint_phase = phase;
            cx.notify();
        }
    }

    pub fn diagnostics(&self, window: &Window) -> MainWindowNoticeWidgetDiagnostics {
        let record = self.record.as_ref();
        let content = record.map(|record| &record.content);
        let measured = self.frame_size.get();
        MainWindowNoticeWidgetDiagnostics {
            widget_instance_id: self.widget_instance_id,
            diagnostic_key: record.map(|record| record.diagnostic_key),
            content_revision: record.map_or(0, |record| record.token.record().revision()),
            variant: content.map_or(NoticeVariant::Info, |content| content.variant),
            dismissal: content.map_or(NoticeDismissal::Dismissible, |content| content.dismissal),
            visible: record.is_some(),
            visibility: if record.is_none() {
                MainWindowNoticeVisibility::Hidden
            } else if self.inert {
                MainWindowNoticeVisibility::Inert
            } else {
                match self.paint_phase {
                    MainWindowNoticePaintPhase::Entering => MainWindowNoticeVisibility::Entering,
                    MainWindowNoticePaintPhase::Visible => MainWindowNoticeVisibility::Visible,
                    MainWindowNoticePaintPhase::Leaving => MainWindowNoticeVisibility::Leaving,
                }
            },
            detail_present: content.is_some_and(|content| !content.detail().as_str().is_empty()),
            command_count: content.map_or(0, |content| content.commands().count()),
            allocated_inline: f32::from(measured.width).round().clamp(0., u16::MAX as f32) as u16,
            allocated_block: f32::from(measured.height)
                .round()
                .clamp(0., u16::MAX as f32) as u16,
            overflow: self.detail_scroll.max_offset().height > px(0.),
            scroll_offset: f32::from(-self.detail_scroll.offset().y).ceil() as i32,
            selection_present: self
                .detail_state
                .selection()
                .is_some_and(|selection| !selection.range().is_empty()),
            focus: self.focus_kind(window),
            replacements: self.replacements,
        }
    }

    fn reconcile_command_focuses(&mut self, cx: &mut Context<Self>) {
        let Some(record) = self.record.as_ref() else {
            self.command_focuses.clear();
            return;
        };
        let mut retained = Vec::with_capacity(record.content.commands().count());
        for command in record.content.commands() {
            let focus = self
                .command_focuses
                .iter()
                .find(|(id, _)| id == &command.id)
                .map(|(_, focus)| focus.clone())
                .unwrap_or_else(|| cx.focus_handle());
            retained.push((command.id, focus));
        }
        self.command_focuses = retained;
    }

    fn rotate_scrollbar_owner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scrollbar_mount_generation = self.scrollbar_mount_generation.saturating_add(1);
        let replacement = ScrollbarOwnerKey {
            owner_id: ScrollbarOwnerId::new(self.widget_instance_id),
            mount_generation: ScrollbarMountGeneration::new(self.scrollbar_mount_generation),
        };
        let should_mount = !self.inert
            && self
                .record
                .as_ref()
                .is_some_and(|record| !record.content.detail().as_str().is_empty());
        if self.scrollbar_state.current_owner().is_some() {
            if should_mount {
                self.scrollbar_state
                    .replace_owner(self.scrollbar_owner, replacement, window, cx);
            } else {
                self.scrollbar_state
                    .unmount_viewport(self.scrollbar_owner, window, cx);
            }
        }
        if should_mount && self.scrollbar_state.current_owner().is_none() {
            self.scrollbar_state.mount(replacement);
        }
        self.scrollbar_owner = replacement;
    }

    fn reconcile_focus(
        &self,
        window: &mut Window,
        detail_was_focused: bool,
        close_was_focused: bool,
        focused_command: Option<&NoticeCommandId>,
    ) {
        let Some(record) = self.record.as_ref() else {
            self.safe_focus.focus(window);
            return;
        };
        if self.inert {
            self.safe_focus.focus(window);
            return;
        }
        if (detail_was_focused && record.content.detail().as_str().is_empty())
            || (close_was_focused && record.content.dismissal == NoticeDismissal::Persistent)
            || focused_command
                .is_some_and(|command| !self.command_focuses.iter().any(|(id, _)| id == command))
        {
            self.focus_fallback(window);
        }
    }

    fn focus_fallback(&self, window: &mut Window) {
        if let Some((_, focus)) = self.command_focuses.iter().find(|(id, _)| {
            self.record
                .as_ref()
                .and_then(|record| record.content.commands().find(|command| command.id == *id))
                .is_some_and(|command| command.state() == NoticeCommandState::Enabled)
        }) {
            focus.focus(window);
        } else if self
            .record
            .as_ref()
            .is_some_and(|record| record.content.dismissal == NoticeDismissal::Dismissible)
        {
            self.close_focus.focus(window);
        } else {
            self.safe_focus.focus(window);
        }
    }

    fn focus_kind(&self, window: &Window) -> MainWindowNoticeControlFocus {
        if self.detail_focus.is_focused(window) {
            MainWindowNoticeControlFocus::Detail
        } else if self.close_focus.is_focused(window) {
            MainWindowNoticeControlFocus::Close
        } else if self
            .command_focuses
            .iter()
            .any(|(_, focus)| focus.is_focused(window))
        {
            MainWindowNoticeControlFocus::Command
        } else if self.safe_focus.is_focused(window) {
            MainWindowNoticeControlFocus::SafeTarget
        } else {
            MainWindowNoticeControlFocus::None
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn request_dismissal_for_test(
        &mut self,
        expected: NoticeVisibleToken,
        cx: &mut Context<Self>,
    ) {
        self.dismiss(expected, cx);
    }

    fn dismiss(&mut self, expected: NoticeVisibleToken, cx: &mut Context<Self>) {
        if self.inert || self.dismissal_pending {
            return;
        }
        let Some(record) = self.record.as_ref() else {
            return;
        };
        if record.token != expected {
            return;
        }
        if record.content.dismissal != NoticeDismissal::Dismissible {
            return;
        }
        self.dismissal_pending = true;
        let event = MainWindowNoticeWidgetEvent::Dismiss(expected);
        (self.on_event)(event.clone());
        cx.emit(event);
        cx.notify();
    }

    fn command(
        &mut self,
        expected: NoticeVisibleToken,
        id: NoticeCommandId,
        cx: &mut Context<Self>,
    ) {
        if self.inert {
            return;
        }
        let Some(record) = self.record.as_ref() else {
            return;
        };
        if record.token != expected {
            return;
        }
        let Some(command) = record.content.commands().find(|command| command.id == id) else {
            return;
        };
        if command.state() != NoticeCommandState::Enabled {
            return;
        }
        let event = MainWindowNoticeWidgetEvent::Command {
            token: expected,
            command: id,
        };
        (self.on_event)(event.clone());
        cx.emit(event);
    }

    fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inert {
            return;
        }
        if let Some(detail) = self
            .record
            .as_ref()
            .map(|record| record.content.detail().as_str())
        {
            self.detail_state
                .set_selection((!detail.is_empty()).then_some(DetailSelection {
                    anchor: 0,
                    head: detail.len(),
                }));
            self.detail_focus.focus(window);
            cx.notify();
        }
    }

    fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if self.inert {
            return;
        }
        let Some(selection) = self.detail_state.selection().map(DetailSelection::range) else {
            return;
        };
        let Some(detail) = self
            .record
            .as_ref()
            .map(|record| record.content.detail().as_str())
        else {
            return;
        };
        if !selection.is_empty()
            && selection.end <= detail.len()
            && detail.is_char_boundary(selection.start)
            && detail.is_char_boundary(selection.end)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(detail[selection].to_owned()));
        }
    }

    fn extend_detail_selection(
        &mut self,
        key: &str,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inert || !self.detail_focus.is_focused(window) {
            return;
        }
        let Some(detail) = self
            .record
            .as_ref()
            .map(|record| record.content.detail().as_str())
        else {
            return;
        };
        let current = self
            .detail_state
            .selection()
            .unwrap_or(DetailSelection { anchor: 0, head: 0 });
        let head = current.head.min(detail.len());
        if matches!(key, "left" | "right") {
            self.detail_state.reset_navigation();
        }
        let target = match key {
            "left" if !extend && !current.range().is_empty() => current.range().start,
            "right" if !extend && !current.range().is_empty() => current.range().end,
            "left" => previous_boundary(detail, head),
            "right" => next_boundary(detail, head),
            "up" => self.detail_state.vertical_target(head, -1),
            "down" => self.detail_state.vertical_target(head, 1),
            "home" => self.detail_state.line_edge(head, false),
            "end" => self.detail_state.line_edge(head, true),
            _ => return,
        };
        let selection = if extend {
            DetailSelection {
                anchor: current.anchor,
                head: target,
            }
        } else {
            DetailSelection {
                anchor: target,
                head: target,
            }
        };
        self.detail_state.set_selection(Some(selection));
        if let Some(caret) = self.detail_state.caret_vertical_bounds(target) {
            let current = -self.detail_scroll.offset().y;
            let viewport = self.detail_scroll.bounds().size.height;
            let next = if caret.start < current {
                caret.start
            } else if caret.end > current + viewport {
                caret.end - viewport
            } else {
                current
            };
            let next = next.max(px(0.)).min(self.detail_scroll.max_offset().height);
            if next != current {
                self.detail_scroll.set_offset(point(px(0.), -next));
                self.scrollbar_visibility.record_viewport_activity(
                    self.scrollbar_owner,
                    window,
                    cx,
                );
            }
        }
        cx.notify();
    }

    fn command_key_down(
        &mut self,
        token: NoticeVisibleToken,
        command: Option<NoticeCommandId>,
        focus: &FocusHandle,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inert
            || event.is_held
            || event.keystroke.modifiers.modified()
            || !matches!(event.keystroke.key.as_str(), "enter" | "space")
            || !focus.is_focused(window)
        {
            return;
        }
        cx.stop_propagation();
        self.keyboard_press = Some((token, command, event.keystroke.key.clone()));
        self.keyboard_blur = Some(cx.on_blur(focus, window, |this, _, _| {
            this.keyboard_press = None;
        }));
    }

    fn command_key_up(
        &mut self,
        token: NoticeVisibleToken,
        command: Option<NoticeCommandId>,
        event: &gpui::KeyUpEvent,
        cx: &mut Context<Self>,
    ) {
        let Some((pressed_token, pressed_command, pressed_key)) = self.keyboard_press.take() else {
            return;
        };
        self.keyboard_blur = None;
        if self.inert
            || event.keystroke.modifiers.modified()
            || pressed_token != token
            || pressed_command != command
            || pressed_key != event.keystroke.key
        {
            return;
        }
        cx.stop_propagation();
        if let Some(command) = command {
            self.command(token, command, cx);
        } else {
            self.dismiss(token, cx);
        }
    }
}

fn previous_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .take_while(|index| *index < offset)
        .last()
        .unwrap_or(0)
}

fn next_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .find(|index| *index > offset)
        .unwrap_or(text.len())
}
