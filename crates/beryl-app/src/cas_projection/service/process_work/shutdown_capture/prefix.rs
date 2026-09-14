use super::*;

pub(super) struct Prefix {
    after: Option<SyndicThreadId>,
    limit: usize,
    records: BTreeMap<SyndicThreadId, Custody>,
}

impl Prefix {
    pub(super) fn new(after: Option<SyndicThreadId>, limit: usize) -> Self {
        Self {
            after,
            limit,
            records: BTreeMap::new(),
        }
    }

    pub(super) fn insert(&mut self, thread: SyndicThreadId, custody: Custody) {
        if self.after.is_some_and(|after| thread <= after) {
            return;
        }
        self.records.entry(thread).or_default().merge(custody);
        if self.records.len() > self.limit {
            self.records.pop_last();
        }
    }

    pub(super) fn finish(mut self, count: usize) -> (BTreeMap<SyndicThreadId, Custody>, bool) {
        let more = self.records.len() > count;
        if more {
            self.records.pop_last();
        }
        (self.records, more)
    }
}
