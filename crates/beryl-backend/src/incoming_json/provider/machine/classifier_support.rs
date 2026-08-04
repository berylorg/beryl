#[derive(Clone, Copy)]
struct ClassifierProbe {
    candidates: u16,
    length: usize,
}

impl ClassifierProbe {
    const fn new() -> Self {
        Self {
            candidates: 0,
            length: 0,
        }
    }

    fn reset(&mut self, candidates: u16) {
        self.candidates = candidates;
        self.length = 0;
    }

    fn push(&mut self, bytes: &[u8], wires: &[&[u8]]) -> u16 {
        for byte in bytes {
            for (index, wire) in wires.iter().enumerate() {
                let bit = 1_u16 << index;
                if self.candidates & bit != 0 && wire.get(self.length) != Some(byte) {
                    self.candidates &= !bit;
                }
            }
            self.length = self.length.saturating_add(1);
        }
        self.candidates
    }

    const fn exact(self, index: usize, length: usize) -> bool {
        self.candidates & (1_u16 << index) != 0 && self.length == length
    }
}

struct FixedBytes {
    bytes: [u8; FIXED_SCALAR_BYTES],
    len: usize,
}

impl FixedBytes {
    const fn new() -> Self {
        Self {
            bytes: [0; FIXED_SCALAR_BYTES],
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Result<(), ProviderObservationSchemaError> {
        let end = self
            .len
            .checked_add(bytes.len())
            .filter(|end| *end <= self.bytes.len())
            .ok_or(ProviderObservationSchemaError::InvalidString)?;
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }

    fn as_str(&self) -> Result<&str, ProviderObservationSchemaError> {
        std::str::from_utf8(&self.bytes[..self.len])
            .map_err(|_| ProviderObservationSchemaError::InvalidString)
    }
}
