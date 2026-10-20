use super::*;
use crate::main_window::creation::command::theme_font;
use std::future::Future;
use std::pin::Pin;
use std::task::{Poll, Waker};

pub(in crate::main_window) const TOOLBAR_HEIGHT: f32 = 44.;
pub(in crate::main_window) const CONTROL_HEIGHT: f32 = 32.;
pub(in crate::main_window) const TOOLBAR_PADDING: f32 = 12.;
pub(in crate::main_window) const TOOLBAR_GAP: f32 = 6.;
pub(in crate::main_window) const MODEL_WIDTH: f32 = 170.;
pub(in crate::main_window) const CONTEXT_WIDTH: f32 = 180.;
pub(in crate::main_window) const STATUS_DIVIDER_WIDTH: f32 = 1.;
pub(in crate::main_window) const SELECTOR_GAP: f32 = 10.;
pub(in crate::main_window) const AFFORDANCE_GAP: f32 = 4.;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::main_window) struct ShellMetrics {
    pub panel: gpui::Size<gpui::Pixels>,
    pub new_thread_width: gpui::Pixels,
    pub selector_width: gpui::Pixels,
    pub running_width: gpui::Pixels,
    pub new_window_width: gpui::Pixels,
    pub exit_width: gpui::Pixels,
    pub settings_width: gpui::Pixels,
    pub turn_width: gpui::Pixels,
    pub lineage: bool,
    pub discussion: bool,
    pub activity: bool,
    pub activity_width: gpui::Pixels,
    pub new_window: bool,
}

impl ShellMetrics {
    pub(in crate::main_window) fn resolve(
        generation: &AppearanceGeneration,
        panel: gpui::Size<gpui::Pixels>,
        lineage: bool,
        discussion: bool,
        new_window: bool,
        app: &App,
    ) -> Result<Self, String> {
        let environment = app.text_system().font_environment_key();
        let width = |caption: &'static str, role, fallback, weight| {
            let font = theme_font(generation, role, fallback, weight);
            let mut style = gpui::TextStyle::default();
            style.font_weight = gpui::FontWeight(font.weight);
            if let Some(family) = font.family {
                style.font_family = family;
            }
            app.text_system()
                .layout_line_checked(caption, px(font.size), &[style.to_run(caption.len())], None)
                .map(|layout| layout.width)
                .map_err(|error| format!("Pinned control text could not be measured: {error}"))
        };
        let button = |caption| {
            width(caption, "button.secondary.label", 13., 500.).map(|width| width + px(26.))
        };
        let exit_width = ["Exit", "Exiting…", "Quit Anyway"]
            .into_iter()
            .try_fold(px(0.), |left, caption| {
                Ok::<_, String>(left.max(button(caption)?))
            })?;
        let metrics = Self {
            panel,
            new_thread_width: button("New Thread")? + px(CONTROL_HEIGHT + 2.),
            selector_width: width("M", "thread-selector-trigger.title", 13., 600.)?
                + width(
                    "Switch thread",
                    "thread-selector-trigger.flyout-affordance",
                    10.,
                    700.,
                )?
                + width("▾", "thread-selector-trigger.flyout-affordance", 10., 700.)?
                + px(26. + SELECTOR_GAP + AFFORDANCE_GAP),
            running_width: button("Running threads · 18446744073709551615")?,
            new_window_width: button("New Window")?,
            exit_width,
            settings_width: button("Settings")?,
            turn_width: width("M", "status.line.value", 12., 400.)? + px(20.),
            lineage,
            discussion,
            activity: false,
            activity_width: px(0.),
            new_window,
        };
        if environment.is_some() && environment != app.text_system().font_environment_key() {
            return Err("Pinned control font environment changed during measurement.".into());
        }
        Ok(metrics)
    }

    pub(in crate::main_window) fn pinned_height(&self) -> gpui::Pixels {
        px(TOOLBAR_HEIGHT + status_controls::STATUS_STRIP_HEIGHT_PX)
            + if self.discussion {
                px(discussion_status::HEIGHT)
            } else {
                px(0.)
            }
            + if self.lineage {
                px(crate::thread_lineage::THREAD_LINEAGE_HEIGHT_PX)
            } else {
                px(0.)
            }
    }

    pub(in crate::main_window) fn minimum(&self) -> gpui::Size<gpui::Pixels> {
        let count = if self.new_window { 8. } else { 7. };
        let toolbar = px(TOOLBAR_PADDING * 2. + TOOLBAR_GAP * (count - 1.) + CONTROL_HEIGHT * 2.)
            + self.new_thread_width
            + self.selector_width
            + self.running_width
            + self.exit_width
            + self.settings_width
            + if self.new_window {
                self.new_window_width
            } else {
                px(0.)
            };
        gpui::size(
            self.panel
                .width
                .max(toolbar)
                .max(if self.activity {
                    self.activity_width
                } else {
                    px(0.)
                })
                .max(px(MODEL_WIDTH + CONTEXT_WIDTH + STATUS_DIVIDER_WIDTH * 2.) + self.turn_width),
            (self.panel.height * 2.).max(
                self.pinned_height()
                    + self.panel.height
                    + if self.activity {
                        px(crate::widgets::activity_panel::model::MIN_HEIGHT)
                    } else {
                        px(0.)
                    },
            ),
        )
    }

    pub(in crate::main_window) fn composer_height(
        &self,
        content: gpui::Pixels,
        viewport: gpui::Pixels,
    ) -> gpui::Pixels {
        (content + px(22.)).max(self.panel.height).min(
            (viewport * 0.5).min(
                (viewport
                    - self.pinned_height()
                    - if self.activity {
                        px(crate::widgets::activity_panel::model::MIN_HEIGHT)
                    } else {
                        px(0.)
                    })
                .max(px(0.)),
            ),
        )
    }
}

fn activity_minimum_width(
    generation: &AppearanceGeneration,
    app: &App,
) -> Result<gpui::Pixels, String> {
    let environment = app.text_system().font_environment_key();
    let font = theme_font(generation, "activity-panel.key", 11., 500.);
    let mut style = gpui::TextStyle::default();
    style.font_weight = gpui::FontWeight(font.weight);
    if let Some(family) = font.family {
        style.font_family = family;
    }
    let mut width = px(60.);
    for caption in ["Agent", "Activity"] {
        width += app
            .text_system()
            .layout_line_checked(caption, px(font.size), &[style.to_run(caption.len())], None)
            .map_err(|error| format!("Activity keys could not be measured: {error}"))?
            .width;
    }
    if environment.is_some() && environment != app.text_system().font_environment_key() {
        return Err("Activity key font environment changed during measurement.".into());
    }
    Ok(width)
}

pub(in crate::main_window) fn contains(
    outer: gpui::Size<gpui::Pixels>,
    inner: gpui::Size<gpui::Pixels>,
) -> bool {
    outer.width >= inner.width && outer.height >= inner.height
}

fn maximum(
    left: gpui::Size<gpui::Pixels>,
    right: gpui::Size<gpui::Pixels>,
) -> gpui::Size<gpui::Pixels> {
    gpui::size(left.width.max(right.width), left.height.max(right.height))
}

#[derive(Clone)]
pub(in crate::main_window) struct MinimumReady {
    state: Rc<RefCell<MinimumResult>>,
}

#[derive(Default)]
struct MinimumResult {
    result: Option<Result<(), String>>,
    minimized: bool,
    waker: Option<Waker>,
}

impl Future for MinimumReady {
    type Output = Result<(), String>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.borrow_mut();
        if let Some(result) = &state.result {
            Poll::Ready(result.clone())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub(super) struct MinimumFlight {
    metrics: ShellMetrics,
    ready: MinimumReady,
}

impl MainWindowShellRoot {
    #[cfg(feature = "test-faults")]
    pub fn test_minimum_update_status(&self) -> (gpui::Size<gpui::Pixels>, bool) {
        (
            self.minimum_protection,
            self.minimum_flight
                .as_ref()
                .is_some_and(|flight| flight.ready.state.borrow().result.is_none()),
        )
    }

    #[cfg(feature = "test-faults")]
    pub fn test_prepare_panel_minimum(
        &mut self,
        height: gpui::Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let mut metrics = self
            .controller
            .as_ref()
            .ok_or("The original shell is unavailable.")?
            .metrics
            .clone();
        metrics.panel.height = height;
        self.ensure_minimum(metrics, window, cx)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_commit_panel_minimum(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let metrics = self
            .minimum_flight
            .as_ref()
            .ok_or("No prepared minimum")?
            .metrics
            .clone();
        if !self.minimum_current(&metrics) {
            return Err("The prepared minimum is not applicable".into());
        }
        self.commit_metrics(metrics, window, cx);
        Ok(())
    }

    pub(in crate::main_window) fn candidate_metrics(
        &self,
        generation: &AppearanceGeneration,
        panel: gpui::Size<gpui::Pixels>,
        lineage: bool,
        app: &App,
    ) -> Result<ShellMetrics, String> {
        let mut metrics = ShellMetrics::resolve(
            generation,
            panel,
            lineage,
            self.selected_discussion_snapshot(app)
                .is_some_and(|snapshot| snapshot.status().is_some()),
            self.creation.is_some() || self.controller.as_ref().is_some_and(|c| c.is_threadless()),
            app,
        )?;
        metrics.activity = self.activity_visible();
        if metrics.activity {
            metrics.activity_width = activity_minimum_width(generation, app)?;
        }
        Ok(metrics)
    }

    pub(in crate::main_window) fn prepare_minimum(
        &mut self,
        metrics: ShellMetrics,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MinimumReady, String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("The original shell is unavailable.")?;
        let required = maximum(controller.metrics.minimum(), metrics.minimum());
        let state_eligible = match window.is_minimized() {
            Some(false) => true,
            Some(true) => metrics == controller.metrics,
            None => false,
        };
        if let Some(flight) = &self.minimum_flight {
            if flight.metrics == metrics {
                let result = flight.ready.state.borrow();
                if !matches!(result.result.as_ref(), Some(Ok(())))
                    || (state_eligible
                        && contains(self.minimum_protection, required)
                        && contains(window.viewport_size(), required))
                {
                    return Ok(flight.ready.clone());
                }
            }
            if flight.ready.state.borrow().result.is_none() {
                return Err("The original window minimum update is busy.".into());
            }
        }
        if state_eligible
            && contains(self.minimum_protection, required)
            && contains(window.viewport_size(), required)
        {
            let ready = MinimumReady {
                state: Rc::new(RefCell::new(MinimumResult {
                    result: Some(Ok(())),
                    ..Default::default()
                })),
            };
            self.minimum_flight = Some(MinimumFlight {
                metrics,
                ready: ready.clone(),
            });
            return Ok(ready);
        }
        let update = window
            .update_minimum_size(required)
            .map_err(|error| format!("Window minimum admission failed: {error:?}"))?;
        let ready = MinimumReady {
            state: Rc::new(RefCell::new(MinimumResult::default())),
        };
        let completion = ready.clone();
        self.minimum_flight = Some(MinimumFlight {
            metrics,
            ready: ready.clone(),
        });
        cx.spawn(async move |root, cx| {
            let outcome = update.await;
            let minimized = matches!(
                &outcome,
                gpui::WindowMinimumUpdateOutcome::NotApplied(
                    gpui::WindowMinimumUpdateFailure::Minimized
                )
            );
            let result = match outcome {
                gpui::WindowMinimumUpdateOutcome::Applied {
                    minimum,
                    client_size,
                } if minimum == required && contains(client_size, required) => Ok(()),
                outcome => Err(format!(
                    "Window minimum preparation did not apply: {outcome:?}"
                )),
            };
            {
                let mut state = completion.state.borrow_mut();
                state.result = Some(result.clone());
                state.minimized = minimized;
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
            }
            let _ = root.update(cx, |root, cx| {
                if result.is_ok()
                    && root
                        .minimum_flight
                        .as_ref()
                        .is_some_and(|flight| Rc::ptr_eq(&flight.ready.state, &completion.state))
                {
                    root.minimum_protection = required;
                }
                cx.notify();
            });
        })
        .detach();
        Ok(ready)
    }

    pub(in crate::main_window) fn prepare_appearance_minimum(
        &mut self,
        metrics: ShellMetrics,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MinimumReady, String> {
        if self.minimum_flight.as_ref().is_some_and(|flight| {
            matches!(flight.ready.state.borrow().result.as_ref(), Some(Err(_)))
        }) {
            self.minimum_flight = None;
        }
        self.prepare_minimum(metrics, window, cx)
    }

    pub(in crate::main_window) fn minimum_current(&self, metrics: &ShellMetrics) -> bool {
        contains(self.minimum_protection, metrics.minimum())
            && contains(self.minimum_viewport, metrics.minimum())
            && (self
                .controller
                .as_ref()
                .is_some_and(|controller| controller.metrics == *metrics)
                || self.minimum_flight.as_ref().is_some_and(|flight| {
                    flight.metrics == *metrics
                        && matches!(flight.ready.state.borrow().result.as_ref(), Some(Ok(())))
                }))
    }

    pub(super) fn ensure_minimum(
        &mut self,
        metrics: ShellMetrics,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let ready = self.prepare_minimum(metrics.clone(), window, cx)?;
        let state = ready.state.borrow();
        match &state.result {
            Some(Ok(())) => Ok(self.minimum_current(&metrics)
                && window.is_minimized().is_some()
                && (self
                    .controller
                    .as_ref()
                    .is_some_and(|controller| controller.metrics == metrics)
                    || window.is_minimized() == Some(false))),
            Some(Err(_)) if state.minimized => Ok(false),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(false),
        }
    }

    pub(in crate::main_window) fn commit_metrics(
        &mut self,
        metrics: ShellMetrics,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let minimum = metrics.minimum();
        let unchanged = self
            .controller
            .as_ref()
            .is_some_and(|controller| controller.metrics == metrics);
        if let Some(controller) = self.controller.as_mut() {
            controller.minimum_size = minimum;
            controller.metrics = metrics;
        }
        if unchanged
            && self
                .minimum_flight
                .as_ref()
                .is_some_and(|flight| flight.ready.state.borrow().result.is_none())
        {
            return;
        }
        self.minimum_flight = None;
        if minimum == self.minimum_protection {
            return;
        }
        let Ok(update) = window.update_minimum_size(minimum) else {
            return;
        };
        let ready = MinimumReady {
            state: Rc::new(RefCell::new(MinimumResult::default())),
        };
        let completion = ready.clone();
        let metrics = self.controller.as_ref().unwrap().metrics.clone();
        self.minimum_flight = Some(MinimumFlight { metrics, ready });
        cx.spawn(async move |root, cx| {
            let applied = matches!(update.await, gpui::WindowMinimumUpdateOutcome::Applied { minimum: actual, client_size } if actual == minimum && contains(client_size, minimum));
            completion.state.borrow_mut().result = Some(if applied { Ok(()) } else { Err("Window minimum relaxation did not apply.".into()) });
            if let Some(waker) = completion.state.borrow_mut().waker.take() { waker.wake(); }
            let _ = root.update(cx, |root, cx| {
                if root.minimum_flight.as_ref().is_some_and(|flight| {
                    Rc::ptr_eq(&flight.ready.state, &completion.state)
                }) {
                    if applied { root.minimum_protection = minimum; }
                    root.minimum_flight = None;
                }
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn observe_minimum_restore(&mut self, window: &Window) {
        if window.is_minimized() == Some(false)
            && self
                .minimum_flight
                .as_ref()
                .is_some_and(|flight| flight.ready.state.borrow().minimized)
        {
            self.minimum_flight = None;
        }
    }
}
