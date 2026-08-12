use beryl_home_store::{
    DomainReconciliation, ReadError, ReconciliationReader, RecordCodec, StorageDomain,
};

/// Monotonic exact-side evidence collected from every descriptor-listed natural record.
pub(crate) struct ReconciliationClassification {
    saw_record: bool,
    ambiguous: bool,
    old_possible: bool,
    new_possible: bool,
}

impl ReconciliationClassification {
    pub(crate) const fn new() -> Self {
        Self {
            saw_record: false,
            ambiguous: false,
            old_possible: true,
            new_possible: true,
        }
    }

    pub(crate) fn observe_matches(&mut self, old_matches: bool, new_matches: bool) {
        self.saw_record = true;
        self.ambiguous |= old_matches == new_matches;
        self.old_possible &= old_matches;
        self.new_possible &= new_matches;
    }

    pub(crate) const fn finish(self) -> DomainReconciliation {
        match (
            self.saw_record,
            self.ambiguous,
            self.old_possible,
            self.new_possible,
        ) {
            (true, false, true, false) => DomainReconciliation::ExactOld,
            (true, false, false, true) => DomainReconciliation::ExactNew,
            _ => DomainReconciliation::Collision,
        }
    }
}

/// Incorporates one exact codec family's descriptor-listed records into a domain classification.
///
/// The reconciliation reader can only supply records named by the operation descriptor, so this
/// performs neither a point lookup nor a domain scan. Every named record must authenticate the
/// same side; an empty descriptor and every ambiguous observation fail closed as a collision.
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
