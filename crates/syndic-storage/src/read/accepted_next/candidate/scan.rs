use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ScanByteTotals {
    pub(super) stored: usize,
    pub(super) decoded: usize,
}

impl ScanByteTotals {
    pub(super) fn remaining(self, maximum: usize) -> usize {
        maximum
            .saturating_sub(self.stored)
            .min(maximum.saturating_sub(self.decoded))
    }

    fn add(
        &mut self,
        stored: usize,
        decoded: usize,
        maximum: usize,
    ) -> Result<(), SyndicReadError> {
        self.stored = self
            .stored
            .checked_add(stored)
            .ok_or(SyndicReadError::Invariant(
                "accepted-next stored-byte total overflowed",
            ))?;
        self.decoded = self
            .decoded
            .checked_add(decoded)
            .ok_or(SyndicReadError::Invariant(
                "accepted-next decoded-byte total overflowed",
            ))?;
        if self.stored > maximum || self.decoded > maximum {
            return Err(SyndicReadError::Invariant(
                "accepted-next candidate page exceeded its clamped byte bound",
            ));
        }
        Ok(())
    }
}

impl SyndicStorage {
    pub(super) fn next_scan_record<F: Family>(
        &self,
        store: &HomeStore,
        key: F::Key,
        totals: &mut ScanByteTotals,
        maximum: usize,
        missing: &'static str,
    ) -> Result<F::Value, SyndicReadError> {
        let remaining = totals.remaining(maximum);
        if remaining == 0 {
            return Err(SyndicReadError::Read(
                beryl_home_store::ReadError::BoundExceeded {
                    domain: "syndic",
                    family: F::NAME,
                    maximum,
                    actual: maximum.saturating_add(1),
                },
            ));
        }
        let page = self.page::<F>(
            store,
            CursorRange::closed(key.clone(), key),
            CursorReadLimits::new(1, remaining).expect("accepted-next scan remainder is nonzero"),
        )?;
        totals.add(page.stored_bytes(), page.decoded_bytes(), maximum)?;
        let mut records = page.into_records().into_iter();
        let record = records.next().ok_or(SyndicReadError::Invariant(missing))?;
        if records.next().is_some() {
            return Err(SyndicReadError::Invariant(
                "accepted-next exact scan returned multiple records",
            ));
        }
        Ok(record)
    }
}
