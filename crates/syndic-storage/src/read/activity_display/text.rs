use super::{ActivityDisplayReadError, ActivityDisplayText};

pub(super) const PREFIX: usize = 1024;

#[derive(Default)]
pub(super) struct Utf8 {
    scalar: [u8; 4],
    used: usize,
}
impl Utf8 {
    pub(super) fn byte(&mut self, byte: u8) -> Result<Option<char>, ActivityDisplayReadError> {
        if self.used == self.scalar.len() {
            return Err(ActivityDisplayReadError::Invalid(
                "invalid Activity source UTF-8 scalar",
            ));
        }
        self.scalar[self.used] = byte;
        self.used += 1;
        match std::str::from_utf8(&self.scalar[..self.used]) {
            Ok(text) => {
                let character = text
                    .chars()
                    .next()
                    .ok_or(ActivityDisplayReadError::Invalid(
                        "empty Activity source scalar",
                    ))?;
                self.used = 0;
                Ok(Some(character))
            }
            Err(error) if error.error_len().is_none() && self.used < 4 => Ok(None),
            Err(_) => Err(ActivityDisplayReadError::Invalid(
                "invalid Activity source UTF-8 scalar",
            )),
        }
    }
    pub(super) fn finish(&self) -> Result<(), ActivityDisplayReadError> {
        if self.used == 0 {
            Ok(())
        } else {
            Err(ActivityDisplayReadError::Invalid(
                "incomplete Activity source UTF-8 scalar",
            ))
        }
    }
}

#[derive(Default)]
pub(super) struct Prefix {
    pub(super) bytes: Vec<u8>,
    pub(super) truncated: bool,
    started: bool,
    ended: bool,
}
impl Prefix {
    pub(super) fn first_line(&mut self, character: char) {
        if self.ended {
            return;
        }
        if character == '\n' || character == '\r' {
            if self.started {
                self.ended = true;
            } else {
                self.bytes.clear();
            }
        } else if !self.started && character.is_whitespace() {
        } else {
            self.started = true;
            let mut encoded = [0; 4];
            for byte in character.encode_utf8(&mut encoded).bytes() {
                self.push(byte);
            }
        }
    }
    pub(super) fn single_line(&mut self, byte: u8) {
        if byte == b'\n' || byte == b'\r' || byte == b'\t' {
            self.push(b' ');
        } else if byte >= 32 {
            self.push(byte);
        }
    }
    pub(super) fn push(&mut self, byte: u8) {
        if self.bytes.len() < PREFIX {
            self.bytes.push(byte);
        } else {
            self.truncated = true;
        }
    }
    pub(super) fn take_text(&mut self) -> ActivityDisplayText {
        let mut bytes = std::mem::take(&mut self.bytes);
        if let Err(error) = std::str::from_utf8(&bytes) {
            bytes.truncate(error.valid_up_to());
            self.truncated = true;
        }
        let text = String::from_utf8(bytes)
            .expect("admitted UTF-8 prefix")
            .trim()
            .to_owned()
            .into_boxed_str();
        ActivityDisplayText {
            text,
            truncated: self.truncated,
        }
    }
}

#[derive(Default)]
pub(super) struct Diff {
    prefix: [u8; 4],
    length: usize,
    in_hunk: bool,
    pub(super) additions: u64,
    pub(super) deletions: u64,
}
impl Diff {
    pub(super) fn byte(&mut self, byte: u8) -> Result<(), ActivityDisplayReadError> {
        if byte == b'\n' {
            self.line()?;
        } else if self.length < self.prefix.len() {
            self.prefix[self.length] = byte;
            self.length += 1;
        }
        Ok(())
    }
    pub(super) fn line(&mut self) -> Result<(), ActivityDisplayReadError> {
        let bytes = &self.prefix[..self.length];
        if bytes.starts_with(b"@@") {
            self.in_hunk = true;
        }
        let counter = if bytes.starts_with(b"+") && (self.in_hunk || !bytes.starts_with(b"+++")) {
            Some(&mut self.additions)
        } else if bytes.starts_with(b"-") && (self.in_hunk || !bytes.starts_with(b"---")) {
            Some(&mut self.deletions)
        } else {
            None
        };
        if let Some(counter) = counter {
            *counter = counter
                .checked_add(1)
                .ok_or(ActivityDisplayReadError::Invalid(
                    "diff line count overflow",
                ))?;
        }
        self.length = 0;
        Ok(())
    }
    pub(super) fn finish_file(&mut self) -> Result<(), ActivityDisplayReadError> {
        self.line()?;
        self.in_hunk = false;
        Ok(())
    }
}
