use std::{
    ops::Range,
    time::{Duration, Instant},
};

use gpui::{
    ClipboardItem, ClipboardLimits, Context, FocusHandle, KeyDownEvent, Subscription, Window,
};

use super::{ResidentTranscriptSelection, SyndicTranscriptPanel};

const PAIR_INTERVAL: Duration = Duration::from_millis(600);
const COPY_TEXT_BYTES: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CopyTarget {
    Records(ResidentTranscriptSelection),
    Code {
        id: u64,
        revision: super::super::ProviderRevision,
        root: [u8; 32],
        range: Range<u64>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CopyMethod {
    Markdown,
    Stripped,
}

#[derive(Default)]
pub(super) struct CopyGesture {
    first: Option<(CopyTarget, Instant)>,
}

impl CopyGesture {
    pub(super) fn press(
        &mut self,
        target: CopyTarget,
        held: bool,
        now: Instant,
    ) -> Option<CopyMethod> {
        if held {
            return None;
        }
        if self.first.as_ref().is_some_and(|(first, started)| {
            first == &target
                && now
                    .checked_duration_since(*started)
                    .is_some_and(|elapsed| elapsed <= PAIR_INTERVAL)
        }) {
            self.first = None;
            Some(CopyMethod::Stripped)
        } else {
            self.first = Some((target, now));
            Some(CopyMethod::Markdown)
        }
    }
    pub(super) fn reset(&mut self) {
        self.first = None;
    }
    pub(super) fn invalidate_records(&mut self) {
        if self
            .first
            .as_ref()
            .is_some_and(|(target, _)| matches!(target, CopyTarget::Records(_)))
        {
            self.reset();
        }
    }
}

#[derive(Default)]
pub(super) struct KeyboardCopy {
    pub(super) gesture: CopyGesture,
    focus: Option<FocusHandle>,
    subscription: Option<Subscription>,
    activation: Option<Subscription>,
    pub(super) feedback: &'static str,
    #[cfg(test)]
    pub(super) writer: Option<
        Box<
            dyn FnMut(
                &ClipboardItem,
                ClipboardLimits,
                &mut Context<SyndicTranscriptPanel>,
            ) -> Result<(), ()>,
        >,
    >,
}

pub(super) fn is_copy(event: &KeyDownEvent) -> bool {
    let modifiers = event.keystroke.modifiers;
    modifiers.control
        && !modifiers.alt
        && !modifiers.shift
        && !modifiers.platform
        && event.keystroke.key.eq_ignore_ascii_case("c")
}

impl SyndicTranscriptPanel {
    #[cfg(test)]
    pub(crate) fn set_keyboard_copy_writer_for_test(
        &mut self,
        writer: impl FnMut(&ClipboardItem, ClipboardLimits, &mut Context<Self>) -> Result<(), ()>
        + 'static,
    ) {
        self.keyboard_copy.writer = Some(Box::new(writer));
    }
    pub(super) fn reset_keyboard_copy(&mut self) {
        self.keyboard_copy.gesture.reset();
    }

    pub(super) fn observe_copy_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.keyboard_copy.activation.is_none() {
            let mut active = window.is_window_active();
            self.keyboard_copy.activation = Some(cx.observe_window_activation(
                window,
                move |panel, window, cx| {
                    if window.is_window_active() != active {
                        active = window.is_window_active();
                        panel.keyboard_copy.gesture.reset();
                        cx.notify();
                    }
                },
            ));
        }
    }

    pub(super) fn observe_copy_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.observe_copy_window(window, cx);
        let focus = window.focused(cx);
        if self.keyboard_copy.focus == focus {
            return;
        }
        self.reset_keyboard_copy();
        self.keyboard_copy.subscription = focus.as_ref().map(|focus| {
            let observed = focus.clone();
            cx.on_blur(focus, window, move |panel, window, cx| {
                if window.focused(cx).as_ref() != Some(&observed) {
                    panel.keyboard_copy.gesture.reset();
                    cx.notify();
                }
            })
        });
        self.keyboard_copy.focus = focus;
    }

    pub(super) fn copy_records_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_held {
            return;
        }
        self.observe_copy_focus(window, cx);
        for code in &mut self.code_panels {
            if code.actions.copy.is_some() {
                code.actions.cancel_copy("Copy superseded");
            }
        }
        let Some(selection) = self.host.resident_selection() else {
            self.reset_keyboard_copy();
            self.keyboard_copy.feedback = "Copy unavailable: no eligible selection";
            cx.notify();
            return;
        };
        if self.staged.is_some() {
            self.reset_keyboard_copy();
            self.keyboard_copy.feedback = "Copy unavailable: presentation changing";
            cx.notify();
            return;
        }
        let method = self
            .keyboard_copy
            .gesture
            .press(CopyTarget::Records(selection), false, Instant::now())
            .unwrap();
        let result = self
            .host
            .resident_copy_payload()
            .ok()
            .and_then(|payload| match method {
                CopyMethod::Markdown => Some(payload.markdown),
                CopyMethod::Stripped => payload.plain_text,
            })
            .filter(|text| text.len() <= COPY_TEXT_BYTES)
            .ok_or(())
            .and_then(|text| {
                let text_budget = text.len() * 3 + 2 + 65_536;
                let item = ClipboardItem::new_string(text);
                let limits = ClipboardLimits {
                    total_bytes: text_budget,
                    text_bytes: text_budget,
                    metadata_bytes: 1,
                    image_bytes: 1,
                };
                #[cfg(test)]
                if let Some(writer) = &mut self.keyboard_copy.writer {
                    return writer(&item, limits, cx);
                }
                cx.write_to_clipboard_checked(&item, limits).map_err(|_| ())
            });
        self.keyboard_copy.feedback = if result.is_ok() {
            "Copied"
        } else {
            "Copy unavailable: representation or clipboard refused"
        };
        cx.notify();
    }

    pub(super) fn copy_code_key(
        &mut self,
        id: u64,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_held {
            return;
        }
        self.observe_copy_focus(window, cx);
        for code in &mut self.code_panels {
            if code.actions.copy.is_some() {
                code.actions.cancel_copy("Copy superseded");
            }
        }
        let target = self
            .code_panels
            .iter()
            .find(|code| code.id == id && !code.overridden)
            .and_then(|code| {
                Some(CopyTarget::Code {
                    id,
                    revision: code.viewport.owner.revision,
                    root: code.actions.root?,
                    range: code.actions.selected_range()?,
                })
            });
        let Some(target) = target else {
            self.reset_keyboard_copy();
            if let Some(code) = self
                .code_panels
                .iter_mut()
                .find(|code| code.id == id && !code.overridden)
            {
                code.actions.feedback = "Copy unavailable: current source or selection unavailable";
            } else {
                self.keyboard_copy.feedback = "Copy unavailable: resource changed";
            }
            cx.notify();
            return;
        };
        self.keyboard_copy
            .gesture
            .press(target, false, Instant::now());
        // Both keyboard representations of literal Code use the exact selected source bytes.
        if let Some(code) = self.code_panels.iter_mut().find(|code| code.id == id) {
            code.actions.cancel_copy("");
        }
        self.begin_code_copy(id, true, cx);
    }
}
