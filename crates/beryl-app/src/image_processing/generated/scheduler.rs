use std::collections::{BTreeMap, VecDeque};

pub(super) struct Scheduler<R, S> {
    queue: VecDeque<R>,
    active: BTreeMap<R, bool>,
    queue_limit: usize,
    worker_limit: usize,
    sweep: Option<S>,
    fresh_sweep: bool,
    discovering: bool,
    stopped: bool,
}

pub(super) enum Work<R, S> {
    Resource(R),
    Discover { sweep: Option<S>, available: usize },
    Wait,
    Stop,
}

impl<R: Copy + Ord, S> Scheduler<R, S> {
    pub(super) fn new(queue_limit: usize, worker_limit: usize) -> Self {
        assert!(queue_limit > 0 && worker_limit > 0);
        Self {
            queue: VecDeque::with_capacity(queue_limit),
            active: BTreeMap::new(),
            queue_limit,
            worker_limit,
            sweep: None,
            fresh_sweep: true,
            discovering: false,
            stopped: false,
        }
    }

    pub(super) fn notify(&mut self) {
        if self.stopped {
            return;
        }
        self.fresh_sweep = true;
        for recheck in self.active.values_mut() {
            *recheck = true;
        }
    }

    pub(super) fn next(&mut self) -> Work<R, S> {
        if self.stopped {
            return Work::Stop;
        }
        if self.active.len() < self.worker_limit {
            if let Some(resource) = self.queue.pop_front() {
                assert!(self.active.insert(resource, false).is_none());
                return Work::Resource(resource);
            }
        }
        let available = self.queue_limit - self.queue.len();
        if !self.discovering && available > 0 && (self.sweep.is_some() || self.fresh_sweep) {
            self.discovering = true;
            let sweep = self.sweep.take();
            if sweep.is_none() {
                self.fresh_sweep = false;
            }
            return Work::Discover { sweep, available };
        }
        Work::Wait
    }

    pub(super) fn discovered(&mut self, resources: impl IntoIterator<Item = R>, next: Option<S>) {
        assert!(self.discovering);
        self.discovering = false;
        if self.stopped {
            return;
        }
        for resource in resources {
            if !self.active.contains_key(&resource) && !self.queue.contains(&resource) {
                assert!(self.queue.len() < self.queue_limit);
                self.queue.push_back(resource);
            }
        }
        self.sweep = next;
    }

    pub(super) fn discovery_failed(&mut self) {
        self.discovering = false;
    }

    pub(super) fn discovery_deferred(&mut self, sweep: Option<S>) {
        assert!(self.discovering);
        self.discovering = false;
        if !self.stopped { self.sweep = sweep; }
    }

    pub(super) fn finished(&mut self, resource: R) {
        let recheck = self.active.remove(&resource).expect("owned active resource");
        if recheck && !self.stopped {
            self.fresh_sweep = true;
        }
    }

    pub(super) fn stop(&mut self) {
        self.stopped = true;
        self.queue.clear();
        self.sweep = None;
        self.fresh_sweep = false;
    }

    pub(super) fn counts(&self) -> (usize, usize) {
        (self.queue.len(), self.active.len())
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn idle(&self) -> bool {
        self.queue.is_empty() && self.active.is_empty() && !self.discovering
            && self.sweep.is_none() && !self.fresh_sweep
    }
}
