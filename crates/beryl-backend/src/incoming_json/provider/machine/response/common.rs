struct FixedScalar<const N: usize> {
    bytes: [u8; N],
    len: usize,
    overflowed: bool,
}

impl<const N: usize> FixedScalar<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            overflowed: false,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        if self.overflowed {
            return;
        }
        let Some(end) = self.len.checked_add(bytes.len()) else {
            self.overflowed = true;
            return;
        };
        if end > N {
            self.overflowed = true;
            return;
        }
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
    }

    fn as_str(&self) -> Option<&str> {
        if self.overflowed {
            return None;
        }
        std::str::from_utf8(&self.bytes[..self.len]).ok()
    }
}

struct UserAgentProduct {
    value: FixedScalar<256>,
    started: bool,
    complete: bool,
    invalid_utf8: bool,
}

impl UserAgentProduct {
    const fn new() -> Self {
        Self {
            value: FixedScalar::new(),
            started: false,
            complete: false,
            invalid_utf8: false,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.invalid_utf8 = true;
            return;
        };
        for character in text.chars() {
            if self.complete {
                continue;
            }
            if character.is_whitespace() {
                self.complete = self.started;
                continue;
            }
            self.started = true;
            let mut encoded = [0; 4];
            self.value
                .push(character.encode_utf8(&mut encoded).as_bytes());
        }
    }

    fn as_str(&self) -> Option<&str> {
        if !self.started || self.invalid_utf8 {
            return None;
        }
        self.value.as_str()
    }
}

struct ExactName {
    expected: &'static [u8],
    length: usize,
    matches: bool,
}

impl ExactName {
    const fn new(expected: &'static [u8]) -> Self {
        Self {
            expected,
            length: 0,
            matches: true,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.matches &= self.expected.get(self.length) == Some(byte);
            self.length = self.length.saturating_add(1);
        }
    }

    fn is_exact(&self) -> bool {
        self.matches && self.length == self.expected.len()
    }
}

#[derive(Clone, Copy)]
enum RequiredValueShape {
    Any,
    String,
    Object,
}

#[derive(Clone, Copy)]
struct OrderedField {
    name: &'static [u8],
    shape: RequiredValueShape,
}

impl OrderedField {
    const fn any(name: &'static [u8]) -> Self {
        Self {
            name,
            shape: RequiredValueShape::Any,
        }
    }

    const fn string(name: &'static [u8]) -> Self {
        Self {
            name,
            shape: RequiredValueShape::String,
        }
    }

    const fn object(name: &'static [u8]) -> Self {
        Self {
            name,
            shape: RequiredValueShape::Object,
        }
    }
}
