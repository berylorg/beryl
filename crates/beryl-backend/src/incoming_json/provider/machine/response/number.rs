struct NumberBytes {
    bytes: [u8; 32],
    len: usize,
    overflowed: bool,
}

impl NumberBytes {
    const fn new() -> Self {
        Self {
            bytes: [0; 32],
            len: 0,
            overflowed: false,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        let Some(end) = self.len.checked_add(bytes.len()) else {
            self.overflowed = true;
            return;
        };
        if end > self.bytes.len() {
            self.overflowed = true;
            return;
        }
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
    }

    fn parse_i64(&self) -> Option<i64> {
        if self.overflowed {
            return None;
        }
        std::str::from_utf8(&self.bytes[..self.len])
            .ok()?
            .parse()
            .ok()
    }

    fn parse_u64(&self) -> Option<u64> {
        if self.overflowed {
            return None;
        }
        std::str::from_utf8(&self.bytes[..self.len])
            .ok()?
            .parse()
            .ok()
    }
}
