use super::*;

#[derive(Default)]
pub(super) struct NotificationEnvelopeTail {
    step: u8,
    name_len: usize,
    name_matches: bool,
    integer: bool,
}

impl NotificationEnvelopeTail {
    pub(super) fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        for byte in bytes {
            match self.step {
                1 => {
                    self.name_matches &= b"emittedAtMs".get(self.name_len) == Some(byte);
                    self.name_len = self.name_len.saturating_add(1);
                }
                3 => self.integer &= byte.is_ascii_digit() || *byte == b'-',
                _ => return Err(malformed()),
            }
        }
        Ok(())
    }

    pub(super) fn event(&mut self, event: Event) -> Result<(), MachineError> {
        match (self.step, event) {
            (0 | 4, Event::ContainerEnd(ContainerKind::Object)) => self.step = 5,
            (0, Event::ScalarStart(ScalarKind::Name)) => {
                self.name_matches = true;
                self.step = 1;
            }
            (1, Event::ScalarFragment(ScalarKind::Name)) => {}
            (1, Event::ScalarEnd(ScalarKind::Name)) if self.name_matches && self.name_len == 11 => {
                self.step = 2
            }
            (2, Event::Null) => self.step = 4,
            (2, Event::ScalarStart(ScalarKind::Number)) => {
                self.integer = true;
                self.step = 3;
            }
            (3, Event::ScalarFragment(ScalarKind::Number)) => {}
            (3, Event::ScalarEnd(ScalarKind::Number)) if self.integer => self.step = 4,
            _ => return Err(malformed()),
        }
        Ok(())
    }

    pub(super) fn complete(&self) -> bool {
        self.step == 5
    }
}

fn malformed() -> MachineError {
    ForegroundIngressError::MalformedContextObservation.into()
}
