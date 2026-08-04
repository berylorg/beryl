#[derive(Clone, Copy)]
enum DiscardDisposition {
    UnknownNotification,
    NoOwnerNotification,
    Unavailable(KnownControlFamily),
    Quarantine,
}

struct DiscardMachine {
    disposition: DiscardDisposition,
    depth: u16,
    scalar: DiscardScalar,
    pending: DiscardPending,
    id_seen: bool,
    root_complete: bool,
}

#[derive(Clone, Copy)]
enum DiscardScalar {
    None,
    RootName(ClassifierProbe),
    Other,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DiscardPending {
    None,
    Id,
    Method,
}

impl DiscardMachine {
    const fn new(disposition: DiscardDisposition) -> Self {
        Self {
            disposition,
            depth: 1,
            scalar: DiscardScalar::None,
            pending: DiscardPending::None,
            id_seen: false,
            root_complete: false,
        }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        if let DiscardScalar::RootName(probe) = &mut self.scalar {
            probe.push(bytes, &DISCARD_ROOT_NAMES);
        }
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::ContainerStart(_) => {
                self.consume_pending_value();
                self.depth = self.depth.saturating_add(1);
            }
            Event::ContainerEnd(_) if self.depth == 1 => {
                self.depth = 0;
                self.root_complete = true;
            }
            Event::ContainerEnd(_) if self.depth > 1 => self.depth -= 1,
            Event::ContainerEnd(_) => {}
            Event::ScalarStart(ScalarKind::Name) if self.depth == 1 => {
                let mut probe = ClassifierProbe::new();
                probe.reset((1_u16 << DISCARD_ROOT_NAMES.len()) - 1);
                self.scalar = DiscardScalar::RootName(probe);
            }
            Event::ScalarStart(_) => {
                self.consume_pending_value();
                self.scalar = DiscardScalar::Other;
            }
            Event::ScalarFragment(_) => {}
            Event::ScalarEnd(ScalarKind::Name) => {
                let DiscardScalar::RootName(probe) = self.scalar else {
                    self.scalar = DiscardScalar::None;
                    return;
                };
                self.pending = if probe.exact(0, DISCARD_ROOT_NAMES[0].len()) {
                    DiscardPending::Id
                } else if probe.exact(1, DISCARD_ROOT_NAMES[1].len()) {
                    DiscardPending::Method
                } else {
                    DiscardPending::None
                };
                self.scalar = DiscardScalar::None;
            }
            Event::ScalarEnd(_) => {
                self.consume_pending_value();
                self.scalar = DiscardScalar::None;
            }
            Event::Boolean(_) | Event::Null => self.consume_pending_value(),
        }
    }

    fn consume_pending_value(&mut self) {
        match std::mem::replace(&mut self.pending, DiscardPending::None) {
            DiscardPending::Id => self.id_seen = true,
            DiscardPending::Method => self.disposition = DiscardDisposition::Quarantine,
            DiscardPending::None => {}
        }
    }

    fn finish(&self) -> Result<DecodedIncoming, ForegroundIngressError> {
        if matches!(self.disposition, DiscardDisposition::Quarantine) {
            return Err(ForegroundIngressError::Quarantined);
        }
        if self.id_seen {
            return Err(ForegroundIngressError::UnsupportedServerRequest);
        }
        match self.disposition {
            DiscardDisposition::UnknownNotification | DiscardDisposition::NoOwnerNotification => {
                Ok(DecodedIncoming::DiscardedNotification)
            }
            DiscardDisposition::Unavailable(family) => {
                let _ = family;
                Err(ForegroundIngressError::KnownControlUnavailable)
            }
            DiscardDisposition::Quarantine => unreachable!(),
        }
    }
}

const DISCARD_ROOT_NAMES: [&[u8]; 2] = [b"id", b"method"];
