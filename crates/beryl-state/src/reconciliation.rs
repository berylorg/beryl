use beryl_home_store::{
    DomainReconciliation, ReadError, ReconciliationReader, RecordCodec, StorageDomain,
};

/// Monotonic exact-side evidence collected from every descriptor-listed natural record.
pub(crate) struct ReconciliationClassification {
    saw_record: bool,
    old_possible: bool,
    new_possible: bool,
}

impl ReconciliationClassification {
    pub(crate) const fn new() -> Self {
        Self {
            saw_record: false,
            old_possible: true,
            new_possible: true,
        }
    }

    pub(crate) fn observe_matches(&mut self, old_matches: bool, new_matches: bool) {
        self.saw_record = true;
        self.old_possible &= old_matches;
        self.new_possible &= new_matches;
    }

    pub(crate) const fn finish(self) -> DomainReconciliation {
        match (self.saw_record, self.old_possible, self.new_possible) {
            (true, true, false) => DomainReconciliation::ExactOld,
            (true, false, true) => DomainReconciliation::ExactNew,
            (true, true, true) => DomainReconciliation::ExactUnchanged,
            _ => DomainReconciliation::Collision,
        }
    }
}

pub(crate) fn classify_records<D, R>(
    reader: &ReconciliationReader<'_, D>,
    classification: &mut ReconciliationClassification,
) -> Result<(), ReadError>
where
    D: StorageDomain,
    R: RecordCodec<D>,
    R::Value: PartialEq,
{
    for record in reader.records::<R>()? {
        classification.observe_matches(
            record.current() == record.old(),
            record.current() == record.new(),
        );
    }
    Ok(())
}
