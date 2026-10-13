use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::super::narrative::{NarrativeCopyBinding, NarrativeSelection};
use super::*;

pub(in super::super) enum NarrativeCopyPart {
    Mapped(String),
    Literal(String),
}

pub(in super::super) struct NarrativeCopyCapture {
    pub(in super::super) selection: NarrativeSelection,
    pub(in super::super) parts: Vec<NarrativeCopyPart>,
    pub(in super::super) bindings: Vec<NarrativeCopyBinding>,
}

pub(super) struct LiteralCopy {
    serial: Option<u64>,
    busy: Arc<AtomicBool>,
    cancelled: Option<Arc<AtomicBool>>,
    task: Option<gpui::Task<()>>,
}

impl Default for LiteralCopy {
    fn default() -> Self {
        Self {
            serial: Some(0),
            busy: Arc::new(AtomicBool::new(false)),
            cancelled: None,
            task: None,
        }
    }
}

impl LiteralCopy {
    fn cancel(&mut self) {
        if let Some(cancelled) = self.cancelled.take() {
            cancelled.store(true, Ordering::Release);
        }
        self.task = None;
        self.serial = self.serial.and_then(|serial| serial.checked_add(1));
    }
}

struct LiteralCopySlot(Arc<AtomicBool>);

impl Drop for LiteralCopySlot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn convert(parts: Vec<NarrativeCopyPart>, cancelled: &AtomicBool) -> Result<String, ()> {
    let source_bytes = parts.iter().try_fold(0usize, |bytes, part| {
        let text = match part {
            NarrativeCopyPart::Mapped(text) | NarrativeCopyPart::Literal(text) => text,
        };
        bytes
            .checked_add(text.len())
            .filter(|bytes| *bytes <= COPY_TEXT_BYTES)
            .ok_or(())
    })?;
    let mut output = String::new();
    output.try_reserve_exact(source_bytes).map_err(|_| ())?;
    for part in parts {
        if cancelled.load(Ordering::Acquire) {
            return Err(());
        }
        let text = match part {
            NarrativeCopyPart::Mapped(text) => text,
            NarrativeCopyPart::Literal(text) => {
                crate::transcript_provider::stripped_copy_source(&text)?
            }
        };
        output
            .len()
            .checked_add(text.len())
            .filter(|bytes| *bytes <= COPY_TEXT_BYTES)
            .ok_or(())?;
        output.push_str(&text);
    }
    if output.is_empty() || cancelled.load(Ordering::Acquire) {
        return Err(());
    }
    Ok(output)
}

impl SyndicTranscriptPanel {
    pub(in super::super) fn cancel_literal_copy(&mut self) {
        self.keyboard_copy.literal.cancel();
    }

    pub(super) fn begin_literal_copy(
        &mut self,
        capture: NarrativeCopyCapture,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(serial) = self.keyboard_copy.literal.serial else {
            self.keyboard_copy.feedback = "Copy unavailable";
            cx.notify();
            return;
        };
        let Some(lease) = self.save_admission.acquire() else {
            self.keyboard_copy.feedback = "Copy unavailable: worker capacity";
            cx.notify();
            return;
        };
        if self
            .keyboard_copy
            .literal
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.keyboard_copy.feedback = "Copy unavailable: previous work retiring";
            cx.notify();
            return;
        }
        let slot = LiteralCopySlot(self.keyboard_copy.literal.busy.clone());
        let cancelled = Arc::new(AtomicBool::new(false));
        self.keyboard_copy.literal.cancelled = Some(cancelled.clone());
        let worker_cancelled = cancelled.clone();
        let focus = window.focused(cx);
        let active = window.is_window_active();
        let NarrativeCopyCapture {
            selection,
            parts,
            bindings,
        } = capture;
        let work = cx.background_executor().spawn(async move {
            let result = convert(parts, &worker_cancelled);
            (result, slot, lease)
        });
        self.keyboard_copy.feedback = "Copying";
        self.keyboard_copy.literal.task = Some(cx.spawn_in(window, async move |weak, cx| {
            let (result, _slot, _lease) = work.await;
            let _ = weak.update_in(cx, |panel, window, cx| {
                if panel.keyboard_copy.literal.serial != Some(serial)
                    || cancelled.load(Ordering::Acquire)
                {
                    return;
                }
                let current = window.focused(cx) == focus
                    && window.is_window_active() == active
                    && panel.save_admission.suspended() == Some(false)
                    && panel.character_geometry_current(window)
                    && panel.narrative_copy_capture_current(&selection, &bindings);
                if !panel.character_geometry_current(window) {
                    panel.close_invalid_geometry();
                }
                let result = if current {
                    result.and_then(|text| {
                        panel.write_keyboard_clipboard(&ClipboardItem::new_string(text), cx)
                    })
                } else {
                    Err(())
                };
                panel.keyboard_copy.feedback = if result.is_ok() {
                    "Copied"
                } else {
                    "Copy unavailable"
                };
                panel.keyboard_copy.literal.cancelled = None;
                panel.keyboard_copy.literal.task = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/syndic_transcript_panel/literal_copy.rs"
    ));
}
