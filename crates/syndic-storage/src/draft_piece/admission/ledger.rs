use std::sync::{Arc, Mutex};

use crate::codec::Family;

use super::{DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, DraftMarkerAdmissionSchemaErrorV1};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DraftMarkerAdmissionWorkSnapshotV1 {
    pub point_attempts: u64,
    pub stored_node_acquisitions: u64,
    pub stored_node_emissions: u64,
    pub encoded_bytes: u64,
    pub peak_reserved_bytes: u64,
}

#[derive(Debug)]
struct WorkState {
    maximum_bytes: u64,
    maximum_points: u64,
    maximum_nodes: u64,
    reserved_nodes: u64,
    reserved_bytes: u64,
    snapshot: DraftMarkerAdmissionWorkSnapshotV1,
}

#[derive(Clone, Debug)]
pub(crate) struct AdmissionWorkLedger(Arc<Mutex<WorkState>>);

pub(crate) struct PointReservation {
    work: AdmissionWorkLedger,
    maximum_bytes: u64,
    reserved_node: bool,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Debug)]
pub struct DraftMarkerAdmissionWorkDiagnosticsV1(AdmissionWorkLedger);

#[cfg(feature = "test-faults")]
impl DraftMarkerAdmissionWorkDiagnosticsV1 {
    pub fn snapshot(&self) -> DraftMarkerAdmissionWorkSnapshotV1 {
        self.0.snapshot()
    }
}

impl AdmissionWorkLedger {
    pub(crate) fn family_maximum<F: Family>() -> u64 {
        if F::NAME == super::DraftMarkerAdmissionHeadsFamily::NAME {
            756
        } else if F::NAME == super::DraftMarkerAdmissionCapacityFamily::NAME {
            89
        } else {
            (F::MAX_KEY_BYTES + F::MAX_VALUE_BYTES) as u64
        }
    }

    pub(crate) fn family_value_limit<F: Family>() -> usize {
        (Self::family_maximum::<F>() as usize - F::MAX_KEY_BYTES)
            + beryl_home_store::RECORD_VERSION_BYTES
    }
    pub(crate) fn new(maximum_bytes: u64) -> Self {
        Self(Arc::new(Mutex::new(WorkState {
            maximum_bytes: maximum_bytes.min(DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES),
            maximum_points: 512,
            maximum_nodes: 256,
            reserved_nodes: 0,
            reserved_bytes: 0,
            snapshot: DraftMarkerAdmissionWorkSnapshotV1::default(),
        })))
    }

    pub(crate) fn assignment(maximum_bytes: u64) -> Self {
        let work = Self::new(maximum_bytes);
        {
            let mut state = work.0.lock().expect("admission work ledger lock");
            state.maximum_points = 153;
            state.maximum_nodes = 140;
        }
        work
    }

    pub(crate) fn snapshot(&self) -> DraftMarkerAdmissionWorkSnapshotV1 {
        self.0.lock().expect("admission work ledger lock").snapshot
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn diagnostics(&self) -> DraftMarkerAdmissionWorkDiagnosticsV1 {
        DraftMarkerAdmissionWorkDiagnosticsV1(self.clone())
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn lower_limit(&self, maximum_bytes: u64) {
        let mut state = self.0.lock().expect("admission work ledger lock");
        state.maximum_bytes = state.maximum_bytes.min(maximum_bytes);
    }

    pub(crate) fn reserve_point(
        &self,
        maximum_bytes: u64,
    ) -> Result<PointReservation, DraftMarkerAdmissionSchemaErrorV1> {
        self.reserve(maximum_bytes, false)
    }

    pub(crate) fn reserve_node_point(
        &self,
        maximum_bytes: u64,
    ) -> Result<PointReservation, DraftMarkerAdmissionSchemaErrorV1> {
        self.reserve(maximum_bytes, true)
    }

    fn reserve(
        &self,
        maximum_bytes: u64,
        stored_node: bool,
    ) -> Result<PointReservation, DraftMarkerAdmissionSchemaErrorV1> {
        let mut state = self.0.lock().expect("admission work ledger lock");
        if maximum_bytes == 0
            || state.snapshot.point_attempts >= state.maximum_points
            || (stored_node
                && state.snapshot.stored_node_acquisitions
                    + state.snapshot.stored_node_emissions
                    + state.reserved_nodes
                    >= state.maximum_nodes)
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge);
        }
        let peak = state
            .snapshot
            .encoded_bytes
            .checked_add(state.reserved_bytes)
            .and_then(|bytes| bytes.checked_add(maximum_bytes))
            .filter(|bytes| *bytes <= state.maximum_bytes)
            .ok_or(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge)?;
        state.reserved_bytes += maximum_bytes;
        state.reserved_nodes += u64::from(stored_node);
        state.snapshot.point_attempts += 1;
        state.snapshot.peak_reserved_bytes = state.snapshot.peak_reserved_bytes.max(peak);
        Ok(PointReservation {
            work: self.clone(),
            maximum_bytes,
            reserved_node: stored_node,
        })
    }

    pub(crate) fn charge_emit(
        &self,
        bytes: u64,
        stored_node: bool,
    ) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        self.charge(bytes, stored_node)
    }

    pub(crate) fn charge_delete(
        &self,
        bytes: u64,
    ) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        self.charge(bytes, false)
    }

    fn charge(
        &self,
        bytes: u64,
        stored_node: bool,
    ) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        let mut state = self.0.lock().expect("admission work ledger lock");
        let total = state
            .snapshot
            .encoded_bytes
            .checked_add(bytes)
            .and_then(|bytes| bytes.checked_add(state.reserved_bytes))
            .filter(|bytes| *bytes <= state.maximum_bytes)
            .ok_or(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge)?;
        if stored_node
            && state.snapshot.stored_node_acquisitions
                + state.snapshot.stored_node_emissions
                + state.reserved_nodes
                >= state.maximum_nodes
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge);
        }
        state.snapshot.encoded_bytes += bytes;
        state.snapshot.stored_node_emissions += u64::from(stored_node);
        state.snapshot.peak_reserved_bytes = state.snapshot.peak_reserved_bytes.max(total);
        Ok(())
    }

    pub(crate) fn point<F: Family, E: From<DraftMarkerAdmissionSchemaErrorV1>>(
        &self,
        key: &F::Key,
        maximum_bytes: u64,
        read: impl FnOnce() -> Result<Option<F::Value>, E>,
    ) -> Result<Option<F::Value>, E> {
        let reservation = self.reserve_point(maximum_bytes)?;
        let value = read()?;
        let key_bytes = F::encode_key(key)
            .map_err(|_| DraftMarkerAdmissionSchemaErrorV1::InvalidHead)?
            .len() as u64;
        let value_bytes = value
            .as_ref()
            .map(F::encode_value)
            .transpose()
            .map_err(|_| DraftMarkerAdmissionSchemaErrorV1::InvalidHead)?
            .map_or(0, |bytes| bytes.len() as u64);
        reservation.finish(key_bytes + value_bytes, false)?;
        Ok(value)
    }
}

impl PointReservation {
    pub(crate) fn finish(
        mut self,
        actual_bytes: u64,
        stored_node: bool,
    ) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        let mut state = self.work.0.lock().expect("admission work ledger lock");
        if actual_bytes > self.maximum_bytes || (stored_node && !self.reserved_node) {
            return Err(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge);
        }
        state.reserved_bytes -= self.maximum_bytes;
        state.reserved_nodes -= u64::from(self.reserved_node);
        self.maximum_bytes = 0;
        self.reserved_node = false;
        state.snapshot.encoded_bytes += actual_bytes;
        state.snapshot.stored_node_acquisitions += u64::from(stored_node);
        Ok(())
    }
}

impl Drop for PointReservation {
    fn drop(&mut self) {
        if self.maximum_bytes != 0 {
            let mut state = self.work.0.lock().expect("admission work ledger lock");
            state.reserved_bytes -= self.maximum_bytes;
            state.reserved_nodes -= u64::from(self.reserved_node);
            state.snapshot.encoded_bytes += self.maximum_bytes;
            state.snapshot.stored_node_acquisitions += u64::from(self.reserved_node);
        }
    }
}
