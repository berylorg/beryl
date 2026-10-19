use super::*;
use beryl_backend::ThreadReadMetadata;
use beryl_model::CasThreadId;

pub(crate) struct ActivityMetadataRequest {
    expected: Fence,
    thread: CasThreadId,
}

pub(crate) struct ActivityMetadataCompletion {
    expected: Fence,
    metadata: display::ActivityAgentMetadata,
}

#[derive(Clone)]
struct Fence {
    active: Weak<AtomicBool>,
    selection: ActivityWindowSelection,
    collection: super::super::collection::ActivityWeakCollection,
    demand: ActivityDemand,
    observation: HomeMutationObservation,
}

impl Fence {
    fn with_current<T>(&self, publish: impl FnOnce() -> T) -> Result<T, ActivityReadError> {
        let collection = self
            .collection
            .upgrade()
            .ok_or(ActivityReadError::Selection)?;
        collection.with_observed(&self.observation, || {
            if !self
                .active
                .upgrade()
                .is_some_and(|active| active.load(Ordering::Acquire))
                || !self.selection.current()
                || !self.demand.current()
            {
                return Err(ActivityReadError::Selection);
            }
            Ok(publish())
        })?
    }
}

impl ActivityPreparedWindow {
    pub(crate) fn metadata_request(&self, thread: &CasThreadId) -> Option<ActivityMetadataRequest> {
        self.with_current(|_| {
            self.pages
                .iter()
                .flat_map(|page| &page.rows)
                .any(|row| row.source().cas_item().turn().thread_id() == thread)
                .then(|| ActivityMetadataRequest {
                    expected: Fence {
                        active: self.active.clone(),
                        selection: self.selection.clone(),
                        collection: self.collection.downgrade(),
                        demand: self.demand.clone(),
                        observation: self.facts.observation.clone(),
                    },
                    thread: thread.clone(),
                })
        })
        .ok()
        .flatten()
    }
}

impl ActivityMetadataRequest {
    pub(crate) fn thread(&self) -> &CasThreadId {
        &self.thread
    }

    pub(crate) fn read(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Option<ThreadReadMetadata>, ActivityReadError> {
        self.expected.with_current(|| ())?;
        let runtime = self
            .expected
            .collection
            .upgrade()
            .ok_or(ActivityReadError::Selection)?
            .runtime();
        Ok(runtime.read_thread_metadata(&self.thread, timeout)?)
    }

    pub(crate) fn current(&self) -> Result<(), ActivityReadError> {
        self.expected.with_current(|| ())
    }

    pub(crate) fn complete(
        &self,
        metadata: &ThreadReadMetadata,
    ) -> Option<ActivityMetadataCompletion> {
        self.try_complete(metadata).ok().flatten()
    }

    pub(crate) fn try_complete(
        &self,
        metadata: &ThreadReadMetadata,
    ) -> Result<Option<ActivityMetadataCompletion>, ActivityReadError> {
        if metadata.thread_id() != &self.thread {
            return Ok(None);
        }
        let Some(value) = metadata.agent_nickname().and_then(|nickname| {
            display::ActivityAgentMetadata::new(self.thread.clone(), nickname, None, None)
        }) else {
            return Ok(None);
        };
        self.expected.with_current(|| ())?;
        Ok(Some(ActivityMetadataCompletion {
            expected: self.expected.clone(),
            metadata: value,
        }))
    }
}

impl ActivityViewport {
    pub(crate) fn metadata_threads(&self) -> Vec<CasThreadId> {
        let mut threads = Vec::new();
        for thread in self
            .resident
            .iter()
            .flat_map(|page| &page.rows)
            .filter(|row| row.source().thread_id() != self.selection.thread_id())
            .map(|row| row.source().cas_item().turn().thread_id())
        {
            if !threads.contains(thread) {
                threads.push(thread.clone());
            }
        }
        threads
    }

    pub(crate) fn metadata_candidate(
        &self,
        attempted: &[CasThreadId],
    ) -> Option<ActivityMetadataRequest> {
        let prepared = self.prepared()?;
        let thread = prepared
            .pages
            .iter()
            .flat_map(|page| &page.rows)
            .filter(|row| row.source().thread_id() != self.selection.thread_id())
            .map(|row| row.source().cas_item().turn().thread_id())
            .find(|thread| {
                !attempted.contains(thread)
                    && !self
                        .metadata
                        .iter()
                        .any(|known| known.thread_id() == *thread)
            })?;
        prepared.metadata_request(thread)
    }

    pub(crate) fn set_metadata(&mut self, completion: &ActivityMetadataCompletion) -> bool {
        self.try_set_metadata(completion).unwrap_or(false)
    }

    pub(crate) fn try_set_metadata(
        &mut self,
        completion: &ActivityMetadataCompletion,
    ) -> Result<bool, ActivityReadError> {
        let ActivityMetadataCompletion { expected, metadata } = completion;
        if !self.collection.as_ref().is_some_and(|collection| {
            expected
                .collection
                .upgrade()
                .is_some_and(|original| collection.same_query(&original))
        }) || !self.demand.current()
            || !self
                .resident
                .iter()
                .flat_map(|page| &page.rows)
                .any(|row| row.source().cas_item().turn().thread_id() == metadata.thread_id())
        {
            return Ok(false);
        }
        let mut pages = VecDeque::new();
        for page in &self.resident {
            let mut rows = page.rows.clone();
            for row in &mut rows {
                if row.source().cas_item().turn().thread_id() == metadata.thread_id() {
                    row.decorate_agent(self.selection.thread_id(), Some(&metadata));
                }
            }
            pages.push_back(Arc::new(ActivityPreparedPage {
                start: page.start,
                rows,
            }));
        }
        expected.with_current(|| {
            self.active.store(false, Ordering::Release);
            self.active = Arc::new(AtomicBool::new(true));
            self.resident = pages;
            self.metadata
                .retain(|prior| prior.thread_id() != metadata.thread_id());
            self.metadata.push_back(metadata.clone());
            while self.metadata.len() > 256 {
                self.metadata.pop_front();
            }
        })?;
        Ok(true)
    }
}
