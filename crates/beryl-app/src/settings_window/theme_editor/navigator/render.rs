use gpui::{
    AnyElement, Bounds, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    Pixels, Render, StatefulInteractiveElement, Styled, canvas, div, point, prelude::FluentBuilder,
    px, size,
};
use gpui_scrollbar::{
    Axis, ScrollbarInteraction, ScrollbarMountGeneration, ScrollbarOwnerId, ScrollbarOwnerKey,
    ScrollbarScrollState, ScrollbarState, ScrollbarStyle, ScrollbarVisibilityPolicy,
};

use super::*;

pub(super) struct ScrollSurface {
    revision: u64,
    owner: ScrollbarOwnerKey,
    state: ScrollbarState,
    visibility: ScrollbarVisibilityPolicy,
    interaction: ScrollbarInteraction,
    geometry: Rc<RefCell<Option<ScrollbarScrollState>>>,
}

pub(super) struct ColumnView {
    parent: Option<ThemeRoleId>,
    pub(super) proxy: FocusHandle,
    pub(super) handles: Vec<(ThemeRoleId, FocusHandle)>,
    rows: Vec<ThemeRoleRow>,
    scroll: ScrollSurface,
}

impl ThemeRoleNavigator {
    pub fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.lifetime.set(None);
        *self.admission.borrow_mut() = None;
        if self.proxy.is_focused(window)
            || self.columns.iter().any(|column| {
                column.proxy.is_focused(window)
                    || column
                        .handles
                        .iter()
                        .any(|(_, focus)| focus.is_focused(window))
            })
        {
            window.blur();
        }
        for column in self.columns.drain(..) {
            column
                .scroll
                .state
                .unmount_viewport(column.scroll.owner, window, cx);
        }
        if let Some(trail) = self.trail.take() {
            trail.state.unmount_viewport(trail.owner, window, cx);
        }
        self.hovered = None;
        cx.notify();
    }

    fn scroll_surface(
        &self,
        parent: Option<Option<ThemeRoleId>>,
        cx: &mut Context<Self>,
    ) -> ScrollSurface {
        let owner = ScrollbarOwnerKey::new(
            ScrollbarOwnerId::new(cx.entity_id().as_u64()),
            ScrollbarMountGeneration::new(self.rebuilds),
        );
        let state = ScrollbarState::new(owner);
        let visibility = state.managed(Rc::new(|_, window, _| window.refresh()));
        let geometry: Rc<RefCell<Option<ScrollbarScrollState>>> = Rc::new(RefCell::new(None));
        let reader = geometry.clone();
        let writer = geometry.clone();
        let pager = geometry.clone();
        let admission = self.admission.clone();
        let lease = self
            .lifetime
            .get()
            .expect("scroll surface has mounted custody");
        let reader_lifetime = self.lifetime.clone();
        let writer_lifetime = self.lifetime.clone();
        let pager_lifetime = self.lifetime.clone();
        let writer_admission = admission.clone();
        let pager_admission = admission.clone();
        let weak = cx.weak_entity();
        let interaction = ScrollbarInteraction::new(
            move || {
                (reader_lifetime.get() == Some(lease) && admission.borrow().is_some())
                    .then(|| *reader.borrow())
                    .flatten()
            },
            move |snapshot, offset| {
                if writer_lifetime.get() != Some(lease) || writer_admission.borrow().is_none() {
                    return;
                }
                if let Some(geometry) = writer.borrow_mut().as_mut() {
                    if geometry.owner != snapshot.owner {
                        return;
                    }
                    match snapshot.axis {
                        Axis::Horizontal => geometry.scroll_offset.x = offset,
                        Axis::Vertical => geometry.scroll_offset.y = offset,
                    }
                }
            },
            move |snapshot, direction, distance| {
                if pager_lifetime.get() != Some(lease) || pager_admission.borrow().is_none() {
                    return;
                }
                if let Some(geometry) = pager.borrow_mut().as_mut() {
                    if geometry.owner != snapshot.owner {
                        return;
                    }
                    let delta = match direction {
                        gpui_scrollbar::ScrollDirection::Backward => -distance,
                        gpui_scrollbar::ScrollDirection::Forward => distance,
                    };
                    geometry.scroll_offset.y =
                        (snapshot.scroll_offset + delta).clamp(px(0.), snapshot.overflow_length());
                }
            },
            |_| {},
            |_| {},
            move |snapshot, window, app| {
                let _ = weak.update(app, |this, cx| {
                    if !this.scroll_lease_is_current(lease) {
                        return;
                    }
                    let offset = f32::from(snapshot.scroll_offset);
                    match &parent {
                        None => {
                            if this
                                .trail
                                .as_ref()
                                .is_some_and(|trail| trail.owner == snapshot.owner)
                            {
                                this.set_horizontal_offset(offset);
                                cx.notify();
                            }
                        }
                        Some(parent) => {
                            if let Some(index) = this.columns.iter().position(|column| {
                                &column.parent == parent && column.scroll.owner == snapshot.owner
                            }) {
                                if this.state.set_offset(index, offset) {
                                    this.reconcile_realized_focus(window, cx);
                                    cx.notify();
                                }
                            }
                        }
                    }
                });
            },
        );
        ScrollSurface {
            revision: self.state.projection.revision,
            owner,
            state,
            visibility,
            interaction,
            geometry,
        }
    }

    fn reconcile_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.lifetime.get().is_none() {
            self.next_mount = self
                .next_mount
                .checked_add(1)
                .expect("navigator mount identity remains representable");
            self.lifetime.set(Some(ThemeRoleScrollLease {
                widget: self.instance,
                mount: self.next_mount,
                revision: self.state.projection.revision,
            }));
        }
        if self.trail.is_none() {
            self.rebuilds = self.rebuilds.saturating_add(1);
            self.trail = Some(self.scroll_surface(None, cx));
            self.release = Some(cx.on_release_in(window, |this, window, cx| {
                this.lifetime.set(None);
                if let Some(trail) = &this.trail {
                    trail.state.unmount_viewport(trail.owner, window, cx);
                }
                for column in &this.columns {
                    column
                        .scroll
                        .state
                        .unmount_viewport(column.scroll.owner, window, cx);
                }
            }));
        }
        if self
            .trail
            .as_ref()
            .is_some_and(|trail| trail.revision != self.state.projection.revision)
        {
            if let Some(old) = self.trail.take() {
                old.state.unmount_viewport(old.owner, window, cx);
            }
            self.rebuilds = self.rebuilds.saturating_add(1);
            self.trail = Some(self.scroll_surface(None, cx));
        }
        let desired: Vec<_> = self
            .state
            .projection
            .columns
            .iter()
            .map(|column| column.parent.clone())
            .collect();
        let mut old = std::mem::take(&mut self.columns);
        for parent in desired {
            if let Some(index) = old.iter().position(|column| column.parent == parent) {
                let mut column = old.remove(index);
                if column.scroll.revision != self.state.projection.revision {
                    column
                        .scroll
                        .state
                        .unmount_viewport(column.scroll.owner, window, cx);
                    self.rebuilds = self.rebuilds.saturating_add(1);
                    column.scroll = self.scroll_surface(Some(parent), cx);
                }
                self.columns.push(column);
            } else {
                self.rebuilds = self.rebuilds.saturating_add(1);
                let scroll = self.scroll_surface(Some(parent.clone()), cx);
                self.columns.push(ColumnView {
                    parent,
                    proxy: cx.focus_handle(),
                    handles: Vec::new(),
                    rows: Vec::new(),
                    scroll,
                });
            }
        }
        for removed in old {
            if removed.proxy.is_focused(window)
                || removed
                    .handles
                    .iter()
                    .any(|(_, focus)| focus.is_focused(window))
            {
                self.proxy.focus(window);
            }
            removed
                .scroll
                .state
                .unmount_viewport(removed.scroll.owner, window, cx);
        }
        self.horizontal_offset = self
            .horizontal_offset
            .clamp(0., (self.content_width() - self.viewport_width).max(0.));
    }

    fn reconcile_realized_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mounted = self.scroll_lease().is_some();
        for index in 0..self.columns.len() {
            let rows = self.state.realized_page(index);
            let column = &mut self.columns[index];
            for (id, handle) in &column.handles {
                if handle.is_focused(window) {
                    if self
                        .state
                        .projection
                        .source
                        .index_of(column.parent.as_ref(), id)
                        .is_some()
                    {
                        self.state.focus = Some(ThemeRoleFocus {
                            revision: self.state.projection.revision,
                            parent: column.parent.clone(),
                            role: id.clone(),
                        });
                        if !rows.iter().any(|row| &row.id == id) {
                            column.proxy.focus(window);
                        }
                    } else {
                        self.proxy.focus(window);
                    }
                }
            }
            column
                .handles
                .retain(|(id, _)| rows.iter().any(|row| &row.id == id));
            for row in &rows {
                if !column.handles.iter().any(|(id, _)| id == &row.id) {
                    column.handles.push((row.id.clone(), cx.focus_handle()));
                }
            }
            column.rows = rows;
            if column.proxy.is_focused(window) && mounted {
                if let Some(logical) = self.state.focus.as_ref().filter(|focus| {
                    focus.revision == self.state.projection.revision
                        && focus.parent == column.parent
                }) {
                    if let Some((_, focus)) =
                        column.handles.iter().find(|(id, _)| id == &logical.role)
                    {
                        focus.focus(window);
                    }
                }
            }
        }
        if self.hovered.as_ref().is_some_and(|hovered| {
            hovered.revision != self.state.projection.revision
                || !self.columns.iter().any(|column| {
                    column.parent == hovered.parent
                        && column.rows.iter().any(|row| row.id == hovered.role)
                })
        }) {
            self.hovered = None;
        }
    }

    fn set_horizontal_offset(&mut self, offset: f32) -> bool {
        if !offset.is_finite() {
            return false;
        }
        let offset = offset.clamp(0., (self.content_width() - self.viewport_width).max(0.));
        let changed = self.horizontal_offset != offset;
        self.horizontal_offset = offset;
        changed
    }

    fn wheel_focus(&self, window: &Window) -> ThemeRoleWheelFocus {
        self.columns
            .iter()
            .position(|column| {
                column.proxy.is_focused(window)
                    || column
                        .handles
                        .iter()
                        .any(|(_, focus)| focus.is_focused(window))
            })
            .map(ThemeRoleWheelFocus::Column)
            .unwrap_or(if self.proxy.is_focused(window) {
                ThemeRoleWheelFocus::Navigator
            } else {
                ThemeRoleWheelFocus::Outside
            })
    }

    pub fn scroll_wheel(
        &mut self,
        lease: ThemeRoleScrollLease,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.scroll_lease_is_current(lease) {
            return false;
        }
        let delta = event.delta.pixel_delta(px(THEME_ROLE_ROW_STRIDE));
        match theme_role_wheel_target(
            self.wheel_focus(window),
            f32::from(delta.x),
            f32::from(delta.y),
            event.modifiers.shift,
        ) {
            ThemeRoleWheelTarget::Page => return false,
            ThemeRoleWheelTarget::Trail => {
                let horizontal = if event.modifiers.shift && delta.x == px(0.) {
                    delta.y
                } else {
                    delta.x
                };
                self.set_horizontal_offset(self.horizontal_offset - f32::from(horizontal));
                if let Some(trail) = &self.trail {
                    trail
                        .visibility
                        .record_viewport_activity(trail.owner, window, cx);
                }
            }
            ThemeRoleWheelTarget::Column(index) => {
                self.state
                    .set_offset(index, self.state.offset(index) - f32::from(delta.y));
                let column = &self.columns[index];
                column
                    .scroll
                    .visibility
                    .record_viewport_activity(column.scroll.owner, window, cx);
                self.reconcile_realized_focus(window, cx);
            }
        }
        cx.notify();
        true
    }

    pub fn column_scroll_offset(&self, parent: Option<&ThemeRoleId>) -> Option<f32> {
        self.state
            .projection
            .columns
            .iter()
            .position(|column| column.parent.as_ref() == parent)
            .map(|index| self.state.offset(index))
    }

    pub fn horizontal_scroll_offset(&self) -> f32 {
        self.horizontal_offset
    }

    fn reveal_logical_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(logical) = self.state.focus.as_ref() else {
            return;
        };
        let Some(index) = self
            .columns
            .iter()
            .position(|column| column.parent == logical.parent)
        else {
            return;
        };
        let left = index as f32 * 184.;
        if left < self.horizontal_offset {
            self.set_horizontal_offset(left);
        } else if left + 176. > self.horizontal_offset + self.viewport_width {
            self.set_horizontal_offset(left + 176. - self.viewport_width);
        }
        self.columns[index].proxy.focus(window);
        self.reconcile_realized_focus(window, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(lease) = self.scroll_lease() else {
            return;
        };
        let key = match event.keystroke.key.as_str() {
            "up" => Some(ThemeRoleNavigationKey::Up),
            "down" => Some(ThemeRoleNavigationKey::Down),
            "home" => Some(ThemeRoleNavigationKey::Home),
            "end" => Some(ThemeRoleNavigationKey::End),
            "left" => Some(ThemeRoleNavigationKey::Left),
            "right" => Some(ThemeRoleNavigationKey::Right),
            _ => None,
        };
        if let Some(key) = key {
            self.state.navigate(key, true);
            self.reveal_logical_focus(window, cx);
            cx.notify();
            cx.stop_propagation();
        } else if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
            if let Some(focus) = self.state.focus.clone() {
                self.activate_role(lease, focus.parent, focus.role, window, cx);
            }
            cx.stop_propagation();
        }
    }

    pub fn activate_role(
        &mut self,
        lease: ThemeRoleScrollLease,
        parent: Option<ThemeRoleId>,
        role: ThemeRoleId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.scroll_lease_is_current(lease) {
            return false;
        }
        let admitted = self.admitted();
        if self
            .state
            .accept_activation(lease.revision, parent, role.clone(), admitted)
        {
            self.reveal_logical_focus(window, cx);
            cx.emit(ThemeRoleNavigatorEvent::RoleActivated {
                revision: lease.revision,
                role,
            });
            cx.notify();
            return true;
        }
        false
    }

    fn column(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let descriptor = self.state.projection.columns[index].clone();
        let revision = self.state.projection.revision;
        let lease = self
            .lifetime
            .get()
            .expect("role column has mounted custody");
        let range = self.state.realized_range(index);
        let offset = self.state.offset(index);
        let rows = self.columns[index].rows.clone();
        let mut rendered = Vec::with_capacity(rows.len());
        for (ordinal, row) in range.zip(rows) {
            let focus = self.columns[index]
                .handles
                .iter()
                .find(|(id, _)| id == &row.id)
                .unwrap()
                .1
                .clone();
            let selected = descriptor.selected.as_ref() == Some(&row.id);
            let focused = focus.is_focused(window);
            let role = row.id.clone();
            let parent = descriptor.parent.clone();
            let tooltip = NavigatorTooltip(row.label.clone(), self.style.clone());
            let focused_tooltip = tooltip.clone();
            let focused_role = row.id.clone();
            let focused_parent = descriptor.parent.clone();
            let weak = cx.weak_entity();
            let tooltip_anchor = canvas(
                move |bounds, window, app| {
                    if focused {
                        window.set_tooltip(gpui::AnyTooltip {
                            view: app.new(|_| focused_tooltip).into(),
                            mouse_position: bounds.bottom_right(),
                            check_visible_and_update: Rc::new(move |_, window, app| {
                                weak.upgrade().is_some_and(|entity| {
                                    let this = entity.read(app);
                                    this.scroll_lease_is_current(lease)
                                        && this
                                            .columns
                                            .iter()
                                            .find(|column| column.parent == focused_parent)
                                            .is_some_and(|column| {
                                                column.handles.iter().any(|(id, focus)| {
                                                    id == &focused_role && focus.is_focused(window)
                                                })
                                            })
                                })
                            }),
                        });
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0();
            let style = self.style.clone();
            let foreground = if selected {
                style.selected_foreground
            } else {
                style.row_foreground
            };
            let mut item = div()
                .id(gpui::SharedString::from(row.id.as_str().to_owned()))
                .absolute()
                .left_0()
                .top(px(ordinal as f32 * THEME_ROLE_ROW_STRIDE - offset))
                .w_full()
                .h(px(THEME_ROLE_ROW_HEIGHT))
                .px(px(8.))
                .gap(px(4.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .text_color(foreground)
                .track_focus(&focus)
                .tab_stop(false)
                .when(selected, |el| el.bg(style.selected_background))
                .hover(move |el| {
                    el.bg(style.hover_background)
                        .text_color(style.hover_foreground)
                })
                .active(move |el| {
                    el.bg(style.pressed_background)
                        .text_color(style.pressed_foreground)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        if this.scroll_lease_is_current(lease)
                            && this.state.accept_activation(
                                revision,
                                parent.clone(),
                                role.clone(),
                                true,
                            )
                        {
                            this.reveal_logical_focus(window, cx);
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_click({
                    let role = row.id.clone();
                    let parent = descriptor.parent.clone();
                    cx.listener(move |this, event, window, cx| {
                        if matches!(event, gpui::ClickEvent::Mouse(_)) {
                            this.activate_role(lease, parent.clone(), role.clone(), window, cx);
                        }
                    })
                })
                .on_hover({
                    let role = row.id.clone();
                    let parent = descriptor.parent.clone();
                    cx.listener(move |this, hovered, _, cx| {
                        if !this.scroll_lease_is_current(lease) {
                            return;
                        }
                        if *hovered {
                            this.hovered = Some(ThemeRoleFocus {
                                revision,
                                parent: parent.clone(),
                                role: role.clone(),
                            });
                        } else if this
                            .hovered
                            .as_ref()
                            .is_some_and(|focus| focus.role == role && focus.parent == parent)
                        {
                            this.hovered = None;
                        }
                        cx.notify();
                    })
                })
                .tooltip(move |_, cx| cx.new(|_| tooltip.clone()).into())
                .child(div().flex_1().min_w_0().truncate().child(row.label))
                .child(tooltip_anchor);
            if focused {
                item = item.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(4.))
                        .border_2()
                        .border_color(self.style.focus_ring),
                );
            }
            if row.has_children {
                item = item.child(
                    div()
                        .flex_none()
                        .size(px(12.))
                        .flex()
                        .items_center()
                        .child("▶"),
                );
            }
            rendered.push(item.into_any_element());
        }
        let scroll = &self.columns[index].scroll;
        let bounds = scroll
            .geometry
            .borrow()
            .map(|geometry| geometry.viewport_bounds)
            .unwrap_or_default();
        *scroll.geometry.borrow_mut() = Some(ScrollbarScrollState {
            owner: scroll.owner,
            viewport_bounds: bounds,
            content_size: size(
                px(174.),
                px(self.state.max_offset(index) + THEME_ROLE_VIEWPORT_HEIGHT),
            ),
            scroll_offset: point(px(0.), px(offset)),
            page_distance: size(px(174.), px(THEME_ROLE_VIEWPORT_HEIGHT)),
        });
        let geometry = scroll.geometry.clone();
        let lifetime = self.lifetime.clone();
        let measure = canvas(
            move |bounds, _, _| {
                if lifetime.get() != Some(lease) {
                    return;
                }
                if let Some(state) = geometry.borrow_mut().as_mut() {
                    state.viewport_bounds = bounds;
                    state.page_distance = bounds.size;
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let stable_id: gpui::SharedString = descriptor
            .parent
            .as_ref()
            .map_or_else(
                || "theme-role-root-column".to_owned(),
                |parent| parent.as_str().to_owned(),
            )
            .into();
        let mut viewport = div()
            .id((gpui::ElementId::Name(stable_id.clone()), "viewport"))
            .relative()
            .w_full()
            .h(px(THEME_ROLE_VIEWPORT_HEIGHT))
            .overflow_hidden()
            .track_focus(&self.columns[index].proxy)
            .tab_stop(false)
            .on_mouse_move(cx.listener(move |this, _, window, cx| {
                if !this.scroll_lease_is_current(lease) {
                    return;
                }
                if let Some(column) = this.columns.get(index) {
                    column.scroll.visibility.record_viewport_activity(
                        column.scroll.owner,
                        window,
                        cx,
                    );
                }
            }))
            .child(measure)
            .children(rendered);
        let scroll = &self.columns[index].scroll;
        if let Some(bar) = gpui_scrollbar::render_scrollbar(
            (gpui::ElementId::Name(stable_id.clone()), "scrollbar"),
            scroll.state.clone(),
            Axis::Vertical,
            ScrollbarStyle {
                thumb_color: self.style.scrollbar_thumb,
                ..Default::default()
            },
            scroll.visibility.clone(),
            scroll.interaction.clone(),
        ) {
            viewport = viewport.child(bar);
        }
        div()
            .id(stable_id)
            .absolute()
            .left(px(index as f32 * 184. - self.horizontal_offset))
            .top_0()
            .w(px(176.))
            .h(px(138.))
            .flex()
            .flex_col()
            .border_1()
            .border_color(self.style.column_border)
            .bg(self.style.column_background)
            .child(
                div()
                    .h(px(28.))
                    .flex_none()
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .text_color(self.style.header_foreground)
                    .child(div().truncate().child(descriptor.header)),
            )
            .child(viewport)
            .into_any_element()
    }
}

impl Render for ThemeRoleNavigator {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.admission.borrow().is_none() {
            if self.lifetime.get().is_some() {
                self.deactivate(window, cx);
            }
            return div()
                .id("theme-role-navigator")
                .w_full()
                .h(px(156.))
                .into_any_element();
        }
        self.reconcile_columns(window, cx);
        self.reconcile_realized_focus(window, cx);
        if self.proxy.is_focused(window) {
            self.state.enter();
            self.reveal_logical_focus(window, cx);
        }
        let columns: Vec<_> = (0..self.columns.len())
            .map(|index| self.column(index, window, cx))
            .collect();
        let trail = self.trail.as_ref().unwrap();
        let lease = self
            .lifetime
            .get()
            .expect("rendered navigator has mounted custody");
        let bounds = trail
            .geometry
            .borrow()
            .map(|state| state.viewport_bounds)
            .unwrap_or_default();
        *trail.geometry.borrow_mut() = Some(ScrollbarScrollState {
            owner: trail.owner,
            viewport_bounds: bounds,
            content_size: size(px(self.content_width()), px(138.)),
            scroll_offset: point(px(self.horizontal_offset), px(0.)),
            page_distance: bounds.size,
        });
        let weak = cx.weak_entity();
        let measure = canvas(
            move |bounds: Bounds<Pixels>, _, app| {
                let _ = weak.update(app, |this, cx| {
                    if !this.scroll_lease_is_current(lease) {
                        return;
                    }
                    let width = f32::from(bounds.size.width);
                    if this.viewport_width != width {
                        this.viewport_width = width;
                        cx.notify();
                    }
                    if let Some(trail) = &this.trail {
                        if let Some(geometry) = trail.geometry.borrow_mut().as_mut() {
                            geometry.viewport_bounds = bounds;
                            geometry.page_distance = bounds.size;
                        }
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let mut trail_element = div()
            .id("theme-role-trail")
            .relative()
            .w_full()
            .h_full()
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(
                move |this, event: &gpui::ScrollWheelEvent, window, cx| {
                    if this.scroll_wheel(lease, event, window, cx) {
                        cx.stop_propagation();
                    }
                },
            ))
            .on_mouse_move(cx.listener(move |this, _, window, cx| {
                if !this.scroll_lease_is_current(lease) {
                    return;
                }
                if let Some(trail) = &this.trail {
                    trail
                        .visibility
                        .record_viewport_activity(trail.owner, window, cx);
                }
            }))
            .child(measure)
            .children(columns);
        if let Some(bar) = gpui_scrollbar::render_scrollbar(
            "theme-role-trail-scrollbar",
            trail.state.clone(),
            Axis::Horizontal,
            ScrollbarStyle {
                thumb_color: self.style.scrollbar_thumb,
                ..Default::default()
            },
            trail.visibility.clone(),
            trail.interaction.clone(),
        ) {
            trail_element = trail_element.child(bar);
        }
        div()
            .id("theme-role-navigator")
            .w_full()
            .min_w_0()
            .h(px(156.))
            .flex_none()
            .p(px(8.))
            .overflow_hidden()
            .border_1()
            .border_color(self.style.navigator_border)
            .rounded(px(6.))
            .bg(self.style.navigator_background)
            .text_color(self.style.foreground)
            .font_family(self.style.font_family.clone())
            .text_size(px(self.style.font_size))
            .font_weight(self.style.font_weight)
            .track_focus(&self.proxy)
            .tab_stop(true)
            .on_key_down(cx.listener(move |this, event, window, cx| {
                if this.scroll_lease_is_current(lease) {
                    this.key_down(event, window, cx);
                }
            }))
            .child(trail_element)
            .into_any_element()
    }
}

#[derive(Clone)]
struct NavigatorTooltip(String, style::NavigatorStyle);

impl Render for NavigatorTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(320.))
            .max_h(px(160.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .border_1()
            .border_color(self.1.tooltip_border)
            .bg(self.1.tooltip_background)
            .text_color(self.1.tooltip_foreground)
            .font_family(self.1.font_family.clone())
            .text_size(px(self.1.font_size))
            .font_weight(self.1.font_weight)
            .child(self.0.clone())
    }
}
