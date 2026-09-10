use std::{
    fmt::{self, Write},
    panic::PanicHookInfo,
    sync::atomic::{AtomicU32, Ordering},
};

pub(super) const MAX_REPORT_BYTES: usize = 4096;
const FORMAT: [u8; 8] = *b"BRYLCR01";
const TRUNCATION: &str = "\n[Report truncated]";

#[repr(C)]
pub(super) struct ReportRecord {
    complete: AtomicU32,
    format: [u8; 8],
    length: u32,
    text: [u8; MAX_REPORT_BYTES],
}

impl ReportRecord {
    pub(super) fn empty() -> Self {
        Self {
            complete: AtomicU32::new(0),
            format: FORMAT,
            length: 0,
            text: [0; MAX_REPORT_BYTES],
        }
    }

    pub(super) fn capture(&mut self, panic: &PanicHookInfo<'_>) {
        let payload = panic
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("[Non-string panic payload]");
        let location = panic
            .location()
            .map(|location| (location.file(), location.line(), location.column()));
        self.write_report(payload, location);
    }

    fn write_report(&mut self, payload: &str, location: Option<(&str, u32, u32)>) {
        let mut output = BoundedText {
            bytes: &mut self.text,
            length: 0,
            truncated: false,
        };
        let _ = writeln!(output, "Beryl {}", env!("CARGO_PKG_VERSION"));
        if let Some((file, line, column)) = location {
            let _ = writeln!(output, "Panic at {file}:{line}:{column}");
        } else {
            let _ = writeln!(output, "Panic location unavailable");
        }
        let _ = write!(output, "\n{payload}");
        self.length = output.finish() as u32;
        self.complete.store(1, Ordering::Release);
    }

    pub(super) fn read(&self) -> Option<String> {
        if self.complete.load(Ordering::Acquire) != 1 || self.format != FORMAT {
            return None;
        }
        let length = self.length as usize;
        if length == 0 || length > MAX_REPORT_BYTES {
            return None;
        }
        std::str::from_utf8(&self.text[..length])
            .ok()
            .map(str::to_owned)
    }
}

struct BoundedText<'a> {
    bytes: &'a mut [u8; MAX_REPORT_BYTES],
    length: usize,
    truncated: bool,
}

impl Write for BoundedText<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.truncated {
            return Ok(());
        }
        let available = MAX_REPORT_BYTES - TRUNCATION.len() - self.length;
        let mut length = text.len().min(available);
        while !text.is_char_boundary(length) {
            length -= 1;
        }
        self.bytes[self.length..self.length + length].copy_from_slice(&text.as_bytes()[..length]);
        self.length += length;
        self.truncated = length != text.len();
        Ok(())
    }
}

impl BoundedText<'_> {
    fn finish(mut self) -> usize {
        if self.truncated {
            self.bytes[self.length..self.length + TRUNCATION.len()]
                .copy_from_slice(TRUNCATION.as_bytes());
            self.length += TRUNCATION.len();
        }
        self.length
    }
}

#[cfg(test)]
#[path = "../../tests/unit/crash_report_record.rs"]
mod tests;
