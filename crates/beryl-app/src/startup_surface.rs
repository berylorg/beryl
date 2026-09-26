use std::{rc::Rc, time::Duration};

use gpui::{
    App, AppContext, Bounds, Context, Entity, EntityId, FocusHandle, KeyDownEvent, Task,
    TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, px, size,
};
use gpui_text_input::{TextInput, TextInputOptions};

mod render;

#[cfg(test)]
#[path = "../tests/unit/startup_surface.rs"]
mod tests;

pub const MAX_DETAIL_BYTES: usize = 4096;
const TRUNCATED: &str = "\n[Detail truncated]";
const BUSY_DURATION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartupAttempt {
    surface: EntityId,
    sequence: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub enum StartupSurfaceEvent {
    Retry(StartupAttempt),
    Exit,
    QuitAnyway(QuitAnywayRequest),
}

#[derive(Debug, Eq, PartialEq)]
pub struct QuitAnywayRequest {
    surface: EntityId,
}

impl QuitAnywayRequest {
    pub fn terminate_process(self) -> ! {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
            let _ = TerminateProcess(GetCurrentProcess(), 1);
        }
        std::process::abort()
    }
}

type Handler = Rc<dyn Fn(StartupSurfaceEvent, &mut App)>;

pub struct StartupSurface {
    detail: Option<Entity<TextInput>>,
    retry_focus: FocusHandle,
    exit_focus: FocusHandle,
    quit_focus: FocusHandle,
    sequence: u64,
    pending: Option<StartupAttempt>,
    exited: bool,
    blocked: bool,
    quit_requested: bool,
    remaining_seconds: u64,
    timer: Option<Task<()>>,
    handler: Handler,
}

impl StartupSurface {
    pub fn attempt(&self, cx: &Context<Self>) -> StartupAttempt {
        StartupAttempt {
            surface: cx.entity_id(),
            sequence: self.sequence,
        }
    }

    pub fn open_failure(
        detail: &str,
        handler: impl Fn(StartupSurfaceEvent, &mut App) + 'static,
        cx: &mut App,
    ) -> Result<WindowHandle<Self>, String> {
        Self::open(Some(bounded_detail(detail)), Rc::new(handler), cx)
    }

    pub fn open_busy(
        handler: impl Fn(StartupSurfaceEvent, &mut App) + 'static,
        cx: &mut App,
    ) -> Result<WindowHandle<Self>, String> {
        Self::open(None, Rc::new(handler), cx)
    }

    fn open(
        detail: Option<String>,
        handler: Handler,
        cx: &mut App,
    ) -> Result<WindowHandle<Self>, String> {
        gpui_text_input::ensure_text_input_bindings(cx);
        let bounds = Bounds::centered(
            None,
            size(px(560.), px(if detail.is_some() { 340. } else { 220. })),
            cx,
        );
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Beryl".into()),
                    ..Default::default()
                }),
                is_resizable: false,
                is_minimizable: false,
                window_min_size: Some(bounds.size),
                ..Default::default()
            },
            move |window, cx| {
                let surface = cx.new(|cx| Self::new(detail, handler, window, cx));
                let weak = surface.downgrade();
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = weak.update(cx, |surface, cx| surface.request_exit(cx));
                    false
                });
                surface
            },
        )
        .map_err(|error| error.to_string())
    }

    fn new(
        detail: Option<String>,
        handler: Handler,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let retry_focus = cx.focus_handle();
        let exit_focus = cx.focus_handle();
        let quit_focus = cx.focus_handle();
        let detail = detail.map(|detail| {
            cx.new(|cx| {
                TextInput::new_with_options(
                    detail,
                    "",
                    TextInputOptions::multiline()
                        .with_read_only(true)
                        .with_undo_limit(0)
                        .with_undo_byte_limit(0),
                    cx,
                )
            })
        });
        if detail.is_some() {
            retry_focus.focus(window);
        } else {
            exit_focus.focus(window);
        }
        let timer = detail.is_none().then(|| {
            let executor = cx.background_executor().clone();
            let deadline = executor.now() + BUSY_DURATION;
            cx.spawn(async move |weak, cx| {
                loop {
                    let remaining = deadline.saturating_duration_since(executor.now());
                    if remaining.is_zero() {
                        let _ = weak.update(cx, |surface, cx| surface.request_exit(cx));
                        break;
                    }
                    executor.timer(remaining.min(Duration::from_secs(1))).await;
                    if weak
                        .update(cx, |surface, cx| {
                            surface.remaining_seconds = deadline
                                .saturating_duration_since(executor.now())
                                .as_secs_f64()
                                .ceil()
                                as u64;
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });
        Self {
            detail,
            retry_focus,
            exit_focus,
            quit_focus,
            sequence: 0,
            pending: None,
            exited: false,
            blocked: false,
            quit_requested: false,
            remaining_seconds: 5,
            timer,
            handler,
        }
    }

    pub fn request_retry(&mut self, cx: &mut Context<Self>) -> Option<StartupAttempt> {
        if self.detail.is_none() || self.pending.is_some() || self.exited || self.blocked {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        let attempt = StartupAttempt {
            surface: cx.entity_id(),
            sequence: self.sequence,
        };
        self.pending = Some(attempt);
        let weak = cx.weak_entity();
        cx.defer(move |cx| {
            let handler = weak
                .update(cx, |surface, _| {
                    (surface.pending == Some(attempt) && !surface.exited)
                        .then(|| surface.handler.clone())
                })
                .ok()
                .flatten();
            if let Some(handler) = handler {
                handler(StartupSurfaceEvent::Retry(attempt), cx);
            }
        });
        cx.notify();
        Some(attempt)
    }

    pub fn complete_failure(
        &mut self,
        attempt: StartupAttempt,
        detail: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.exited || self.blocked || self.pending != Some(attempt) {
            return false;
        }
        let Some(input) = &self.detail else {
            return false;
        };
        input.update(cx, |input, cx| {
            input.set_text(bounded_detail(detail), cx);
        });
        self.pending = None;
        cx.notify();
        true
    }

    pub fn request_exit(&mut self, cx: &mut Context<Self>) {
        if self.exited || self.quit_requested {
            return;
        }
        self.exited = true;
        self.timer = None;
        let handler = self.handler.clone();
        cx.defer(move |cx| handler(StartupSurfaceEvent::Exit, cx));
        cx.notify();
    }

    pub fn block_cleanup(
        &mut self,
        attempt: StartupAttempt,
        detail: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.blocked || self.quit_requested || attempt != self.attempt(cx) {
            return false;
        }
        let Some(input) = &self.detail else {
            return false;
        };
        input.update(cx, |input, cx| input.set_text(bounded_detail(detail), cx));
        input.read(cx).tab_focus_handle().focus(window);
        self.blocked = true;
        self.pending = None;
        cx.notify();
        true
    }

    pub fn request_quit_anyway(&mut self, cx: &mut Context<Self>) {
        if !self.blocked || self.quit_requested {
            return;
        }
        self.quit_requested = true;
        let weak = cx.weak_entity();
        cx.defer(move |cx| {
            let handler = weak
                .update(cx, |surface, _| {
                    (surface.blocked && surface.quit_requested).then(|| surface.handler.clone())
                })
                .ok()
                .flatten();
            if let Some(handler) = handler {
                handler(
                    StartupSurfaceEvent::QuitAnyway(QuitAnywayRequest {
                        surface: weak.entity_id(),
                    }),
                    cx,
                );
            }
        });
        cx.notify();
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || event.is_held {
            return;
        }
        if event.keystroke.key == "tab" {
            let mut focuses = Vec::with_capacity(4);
            if let Some(detail) = &self.detail {
                focuses.push(detail.read(cx).tab_focus_handle());
            }
            if self.detail.is_some() && self.pending.is_none() && !self.exited && !self.blocked {
                focuses.push(self.retry_focus.clone());
            }
            focuses.push(self.exit_focus.clone());
            if self.blocked && !self.quit_requested {
                focuses.push(self.quit_focus.clone());
            }
            let current = focuses.iter().position(|focus| focus.is_focused(window));
            let next = match current {
                Some(index) if modifiers.shift => (index + focuses.len() - 1) % focuses.len(),
                Some(index) => (index + 1) % focuses.len(),
                None => {
                    if modifiers.shift {
                        focuses.len() - 1
                    } else {
                        0
                    }
                }
            };
            focuses[next].focus(window);
        } else {
            return;
        }
        cx.stop_propagation();
        cx.notify();
    }
}

pub(crate) fn bounded_detail(detail: &str) -> String {
    if detail.len() <= MAX_DETAIL_BYTES {
        return detail.to_owned();
    }
    let mut end = MAX_DETAIL_BYTES - TRUNCATED.len();
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    let mut result = String::with_capacity(MAX_DETAIL_BYTES);
    result.push_str(&detail[..end]);
    result.push_str(TRUNCATED);
    result
}
