use super::super::ProviderFrameDecodeError;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Utf8State {
    remaining: u8,
    next_minimum: u8,
    next_maximum: u8,
}

impl Utf8State {
    pub(super) fn push(
        &mut self,
        bytes: &[u8],
        kind: &'static str,
    ) -> Result<(), ProviderFrameDecodeError> {
        for &byte in bytes {
            if self.remaining != 0 {
                if byte < self.next_minimum || byte > self.next_maximum {
                    return Err(ProviderFrameDecodeError::InvalidUtf8 { kind });
                }
                self.remaining -= 1;
                self.next_minimum = 0x80;
                self.next_maximum = 0xbf;
                continue;
            }
            match byte {
                0x00..=0x7f => {}
                0xc2..=0xdf => self.begin(1, 0x80, 0xbf),
                0xe0 => self.begin(2, 0xa0, 0xbf),
                0xe1..=0xec | 0xee..=0xef => self.begin(2, 0x80, 0xbf),
                0xed => self.begin(2, 0x80, 0x9f),
                0xf0 => self.begin(3, 0x90, 0xbf),
                0xf1..=0xf3 => self.begin(3, 0x80, 0xbf),
                0xf4 => self.begin(3, 0x80, 0x8f),
                _ => return Err(ProviderFrameDecodeError::InvalidUtf8 { kind }),
            }
        }
        Ok(())
    }

    pub(super) fn finish(self, kind: &'static str) -> Result<(), ProviderFrameDecodeError> {
        if self.remaining == 0 {
            Ok(())
        } else {
            Err(ProviderFrameDecodeError::InvalidUtf8 { kind })
        }
    }

    fn begin(&mut self, remaining: u8, minimum: u8, maximum: u8) {
        self.remaining = remaining;
        self.next_minimum = minimum;
        self.next_maximum = maximum;
    }
}
