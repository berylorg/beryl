use super::IncomingMessage;
use crate::PreBindControlDiagnostics;

struct ApprovalNode {
    message: IncomingMessage,
    next: Option<Box<Self>>,
}

pub(super) struct PreBindApprovalPrefix {
    newest: Option<Box<ApprovalNode>>,
    ready: Option<Box<ApprovalNode>>,
    capacity: usize,
    current: usize,
    high_water: usize,
    admissions: u64,
    full: u64,
}

impl PreBindApprovalPrefix {
    pub(super) const fn disabled() -> Self {
        Self::new(0)
    }

    pub(super) const fn new(capacity: usize) -> Self {
        Self {
            newest: None,
            ready: None,
            capacity,
            current: 0,
            high_water: 0,
            admissions: 0,
            full: 0,
        }
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.current == 0
    }

    pub(super) fn try_push(&mut self, message: IncomingMessage) -> Result<(), IncomingMessage> {
        if self.current == self.capacity {
            self.full = self.full.saturating_add(1);
            return Err(message);
        }
        self.newest = Some(Box::new(ApprovalNode {
            message,
            next: self.newest.take(),
        }));
        self.current += 1;
        self.high_water = self.high_water.max(self.current);
        self.admissions = self.admissions.saturating_add(1);
        Ok(())
    }

    pub(super) fn back(&self) -> Option<&IncomingMessage> {
        self.newest.as_ref().map(|node| &node.message).or_else(|| {
            let mut node = self.ready.as_deref()?;
            while let Some(next) = node.next.as_deref() {
                node = next;
            }
            Some(&node.message)
        })
    }

    pub(super) fn pop_front(&mut self) -> Option<IncomingMessage> {
        if self.ready.is_none() {
            while let Some(mut node) = self.newest.take() {
                self.newest = node.next.take();
                node.next = self.ready.take();
                self.ready = Some(node);
            }
        }
        let mut node = self.ready.take()?;
        self.ready = node.next.take();
        self.current -= 1;
        Some(node.message)
    }

    pub(super) fn clear(&mut self) {
        clear_nodes(&mut self.newest);
        clear_nodes(&mut self.ready);
        self.current = 0;
    }

    pub(super) const fn diagnostics(&self) -> PreBindControlDiagnostics {
        PreBindControlDiagnostics {
            capacity: self.capacity,
            current: self.current,
            high_water: self.high_water,
            admissions: self.admissions,
            full: self.full,
        }
    }
}

impl Drop for PreBindApprovalPrefix {
    fn drop(&mut self) {
        self.clear();
    }
}

fn clear_nodes(nodes: &mut Option<Box<ApprovalNode>>) {
    while let Some(mut node) = nodes.take() {
        *nodes = node.next.take();
    }
}
