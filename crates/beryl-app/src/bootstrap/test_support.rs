use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Producer {
    DraftActivation,
    DraftPage,
    CatalogCertification,
    SelectedHistory,
    ResourceMetadata,
    RuntimeLaunch,
}

struct State {
    entered: usize,
    returned: usize,
    released: bool,
}

pub(crate) struct ProducerGate {
    state: Mutex<State>,
    changed: Condvar,
}

type Registration = (Producer, [u8; 16], Weak<ProducerGate>);
static GATES: Mutex<Vec<Registration>> = Mutex::new(Vec::new());

pub(crate) struct ProducerHold {
    gate: Arc<ProducerGate>,
}

impl ProducerHold {
    pub(crate) fn install(producer: Producer, identity: [u8; 16]) -> Self {
        let gate = Arc::new(ProducerGate {
            state: Mutex::new(State {
                entered: 0,
                returned: 0,
                released: false,
            }),
            changed: Condvar::new(),
        });
        let mut gates = GATES.lock().unwrap();
        assert!(!gates.iter().any(|(kind, key, old)| *kind == producer
            && *key == identity
            && old.upgrade().is_some()));
        gates.push((producer, identity, Arc::downgrade(&gate)));
        Self { gate }
    }

    pub(crate) fn gate(&self) -> Arc<ProducerGate> {
        self.gate.clone()
    }
}

impl Drop for ProducerHold {
    fn drop(&mut self) {
        self.gate.release();
        GATES
            .lock()
            .unwrap()
            .retain(|(_, _, gate)| !gate.ptr_eq(&Arc::downgrade(&self.gate)));
    }
}

impl ProducerGate {
    pub(crate) fn entered(&self) -> usize {
        self.state.lock().unwrap().entered
    }
    pub(crate) fn returned(&self) -> usize {
        self.state.lock().unwrap().returned
    }
    pub(crate) fn release(&self) {
        self.state.lock().unwrap().released = true;
        self.changed.notify_all();
    }
}

pub(crate) fn checkpoint(producer: Producer, identity: [u8; 16], cancelled: impl Fn() -> bool) {
    let gate = GATES
        .lock()
        .unwrap()
        .iter()
        .find(|(kind, key, _)| *kind == producer && *key == identity)
        .and_then(|(_, _, gate)| gate.upgrade());
    let Some(gate) = gate else {
        return;
    };
    let mut state = gate.state.lock().unwrap();
    state.entered += 1;
    gate.changed.notify_all();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !state.released && !cancelled() {
        assert!(
            Instant::now() < deadline,
            "held {producer:?} producer deadline"
        );
        state = gate
            .changed
            .wait_timeout(state, Duration::from_millis(10))
            .unwrap()
            .0;
    }
    state.returned += 1;
    gate.changed.notify_all();
}
