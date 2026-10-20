mod model;
mod render;
mod style;

pub use model::*;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use beryl_state::ThemeRoleId;
use gpui::{AppContext, Context, FocusHandle, Window};
use gpui_settings_window::SettingsInteractionAdmission;

use crate::theme_runtime::{AdapterFailureClass, AppearanceGeneration};

pub struct ThemeRoleNavigator {
    state: ThemeRoleNavigatorState,
    style: style::NavigatorStyle,
    generation: Arc<AppearanceGeneration>,
    admission: Rc<RefCell<Option<SettingsInteractionAdmission>>>,
    proxy: FocusHandle,
    columns: Vec<render::ColumnView>,
    trail: Option<render::ScrollSurface>,
    release: Option<gpui::Subscription>,
    viewport_width: f32,
    horizontal_offset: f32,
    rebuilds: u64,
    hovered: Option<ThemeRoleFocus>,
    lifetime: Rc<Cell<Option<ThemeRoleScrollLease>>>,
    next_mount: u64,
    instance: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeRoleNavigatorEvent {
    RoleActivated { revision: u64, role: ThemeRoleId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRoleNavigatorDiagnostics {
    pub widget_instance: u64,
    pub projection_revision: u64,
    pub total_schema_roles: usize,
    pub selected_path_depth: usize,
    pub column_count: usize,
    pub visible_rows: usize,
    pub realized_rows: usize,
    pub property_rows: usize,
    pub horizontal_overflow: bool,
    pub vertical_overflow_columns: usize,
    pub overscan: usize,
    pub focused_target_kind: &'static str,
    pub logical_focus_present: bool,
    pub viewport_focus_proxy_present: bool,
    pub popup_anchor_present: bool,
    pub reconciliations: u64,
    pub rebuilds: u64,
}

impl gpui::EventEmitter<ThemeRoleNavigatorEvent> for ThemeRoleNavigator {}

impl ThemeRoleNavigator {
    pub fn mount(
        projection: ThemeRoleProjection,
        generation: Arc<AppearanceGeneration>,
        admission: Rc<RefCell<Option<SettingsInteractionAdmission>>>,
        app: &mut gpui::App,
    ) -> Result<gpui::Entity<Self>, AdapterFailureClass> {
        let state = ThemeRoleNavigatorState::new(projection)?;
        let style = style::NavigatorStyle::prepare(&generation)?;
        Ok(app.new(|cx| Self::from_prepared(state, style, generation, admission, cx)))
    }

    pub fn validate_appearance(
        generation: &AppearanceGeneration,
    ) -> Result<(), AdapterFailureClass> {
        style::NavigatorStyle::prepare(generation).map(|_| ())
    }

    pub fn new(
        projection: ThemeRoleProjection,
        generation: Arc<AppearanceGeneration>,
        admission: Rc<RefCell<Option<SettingsInteractionAdmission>>>,
        cx: &mut Context<Self>,
    ) -> Result<Self, AdapterFailureClass> {
        let state = ThemeRoleNavigatorState::new(projection)?;
        let style = style::NavigatorStyle::prepare(&generation)?;
        Ok(Self::from_prepared(state, style, generation, admission, cx))
    }

    fn from_prepared(
        state: ThemeRoleNavigatorState,
        style: style::NavigatorStyle,
        generation: Arc<AppearanceGeneration>,
        admission: Rc<RefCell<Option<SettingsInteractionAdmission>>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            state,
            style,
            generation,
            admission,
            proxy: cx.focus_handle(),
            columns: Vec::new(),
            trail: None,
            release: None,
            viewport_width: 0.,
            horizontal_offset: 0.,
            rebuilds: 0,
            hovered: None,
            lifetime: Rc::new(Cell::new(None)),
            next_mount: 0,
            instance: cx.entity_id().as_u64(),
        }
    }

    pub fn sync(
        &mut self,
        projection: ThemeRoleProjection,
        generation: Arc<AppearanceGeneration>,
        cx: &mut Context<Self>,
    ) -> Result<(), AdapterFailureClass> {
        let style = style::NavigatorStyle::prepare(&generation)?;
        self.state.reconcile(projection)?;
        if let Some(lease) = self.lifetime.get() {
            self.lifetime.set(Some(ThemeRoleScrollLease {
                mount: lease.mount,
                revision: self.state.projection.revision,
                ..lease
            }));
        }
        self.style = style;
        self.generation = generation;
        cx.notify();
        Ok(())
    }

    pub fn sync_appearance(
        &mut self,
        generation: Arc<AppearanceGeneration>,
        cx: &mut Context<Self>,
    ) -> Result<(), AdapterFailureClass> {
        self.style = style::NavigatorStyle::prepare(&generation)?;
        self.generation = generation;
        cx.notify();
        Ok(())
    }

    pub fn focus_proxy(&self) -> FocusHandle {
        self.proxy.clone()
    }

    pub fn logical_focus(&self) -> Option<&ThemeRoleFocus> {
        self.state.logical_focus()
    }

    fn admitted(&self) -> bool {
        self.admission
            .borrow()
            .as_ref()
            .is_some_and(SettingsInteractionAdmission::is_open)
    }

    pub fn scroll_lease(&self) -> Option<ThemeRoleScrollLease> {
        self.admission.borrow().as_ref()?;
        self.lifetime.get()
    }

    fn scroll_lease_is_current(&self, lease: ThemeRoleScrollLease) -> bool {
        self.admission.borrow().is_some() && self.lifetime.get() == Some(lease)
    }

    pub fn diagnostics(&self, window: &Window, instance: u64) -> ThemeRoleNavigatorDiagnostics {
        let row_focused = self.columns.iter().any(|column| {
            column
                .handles
                .iter()
                .any(|(_, focus)| focus.is_focused(window))
        });
        let proxy_focused = self.proxy.is_focused(window)
            || self
                .columns
                .iter()
                .any(|column| column.proxy.is_focused(window));
        ThemeRoleNavigatorDiagnostics {
            widget_instance: instance,
            projection_revision: self.state.projection.revision,
            total_schema_roles: self.state.projection.source.total_role_count(),
            selected_path_depth: self
                .state
                .projection
                .columns
                .iter()
                .filter(|c| c.selected.is_some())
                .count(),
            column_count: self.columns.len(),
            visible_rows: (0..self.columns.len())
                .map(|i| self.state.visible_range(i).len())
                .sum(),
            realized_rows: self.columns.iter().map(|c| c.handles.len()).sum(),
            property_rows: self.state.projection.property_row_count,
            horizontal_overflow: self.trail.is_some() && self.content_width() > self.viewport_width,
            vertical_overflow_columns: (0..self.columns.len())
                .filter(|i| self.state.max_offset(*i) > 0.)
                .count(),
            overscan: THEME_ROLE_OVERSCAN,
            focused_target_kind: if row_focused {
                "row"
            } else if proxy_focused {
                "viewport-proxy"
            } else {
                "outside"
            },
            logical_focus_present: self.state.focus.is_some(),
            viewport_focus_proxy_present: proxy_focused,
            popup_anchor_present: row_focused || self.hovered.is_some(),
            reconciliations: self.state.reconciliations,
            rebuilds: self.rebuilds,
        }
    }

    fn content_width(&self) -> f32 {
        let count = self.state.projection.columns.len();
        (count as f32 * 176.) + (count.saturating_sub(1) as f32 * 8.)
    }
}
