mod support;

use std::{
    marker::PhantomData,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    time::Duration,
};

use beryl_home_store::{
    CommandCancellation, CommandError, DomainMutation, DomainReader, HomeCommand, MutationBuilder,
    PointReadLimit,
};
use tempfile::tempdir;

use support::{
    committed, open_home, AlphaDomain, BetaDomain, BytesRecord, FixtureMutationError, PutBytes,
};

#[derive(Default)]
struct AssemblyGate {
    release: Mutex<bool>,
    wake: Condvar,
    active: AtomicUsize,
    maximum: AtomicUsize,
}

impl AssemblyGate {
    fn enter(&self) {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        let mut release = self.release.lock().unwrap();
        while !*release {
            release = self.wake.wait(release).unwrap();
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
    }

    fn release(&self) {
        *self.release.lock().unwrap() = true;
        self.wake.notify_all();
    }
}

struct BlockingAssembly<D> {
    gate: Arc<AssemblyGate>,
    entered: mpsc::Sender<&'static str>,
    label: &'static str,
    key: u64,
    reject: bool,
    _typed: PhantomData<fn() -> D>,
}

impl<D> BlockingAssembly<D> {
    fn new(
        gate: Arc<AssemblyGate>,
        entered: mpsc::Sender<&'static str>,
        label: &'static str,
        key: u64,
        reject: bool,
    ) -> Self {
        Self {
            gate,
            entered,
            label,
            key,
            reject,
            _typed: PhantomData,
        }
    }
}

impl<D: beryl_home_store::StorageDomain> DomainMutation<D> for BlockingAssembly<D> {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(self, _reader: &DomainReader<'_, D>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, D>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<BytesRecord<D>>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error> {
        prepared.entered.send(prepared.label).unwrap();
        prepared.gate.enter();
        if prepared.reject {
            return Err(FixtureMutationError::Rejected("blocked fixture rejects"));
        }
        mutations.put::<BytesRecord<D>>(&prepared.key, &prepared.label.as_bytes().to_vec())?;
        Ok(())
    }
}

#[test]
fn writer_assembly_is_serialized_while_typed_reads_continue() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    seed(&store, &alpha);
    let store = Arc::new(store);
    let expected_home = store.home_revision().unwrap();
    let alpha_revision = store.domain_revision(&alpha).unwrap();
    let beta_revision = store.domain_revision(&beta).unwrap();
    let gate = Arc::new(AssemblyGate::default());
    let (entered_tx, entered_rx) = mpsc::channel();

    let first_store = Arc::clone(&store);
    let first_gate = Arc::clone(&gate);
    let first_tx = entered_tx.clone();
    let first_alpha = alpha.clone();
    let first = std::thread::spawn(move || {
        let mut command = HomeCommand::new(expected_home);
        command
            .add(first_alpha.contribution(
                alpha_revision,
                BlockingAssembly::<AlphaDomain>::new(first_gate, first_tx, "first", 2, true),
            ))
            .unwrap();
        first_store.execute(command)
    });
    assert_eq!(
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        "first"
    );

    let second_store = Arc::clone(&store);
    let second_gate = Arc::clone(&gate);
    let second_tx = entered_tx;
    let second = std::thread::spawn(move || {
        let mut command = HomeCommand::new(expected_home);
        command
            .add(beta.contribution(
                beta_revision,
                BlockingAssembly::<BetaDomain>::new(second_gate, second_tx, "second", 3, false),
            ))
            .unwrap();
        second_store.execute(command)
    });

    assert!(entered_rx.recv_timeout(Duration::from_millis(150)).is_err());
    let (read_tx, read_rx) = mpsc::channel();
    let read_store = Arc::clone(&store);
    let read_alpha = alpha.clone();
    std::thread::spawn(move || {
        read_tx
            .send(
                read_store
                    .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                        &read_alpha,
                        &1,
                        PointReadLimit::new(64).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
    });
    assert_eq!(
        read_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        Some(b"seed".to_vec())
    );

    gate.release();
    assert!(matches!(
        first.join().unwrap(),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorAssembly {
                domain: "alpha",
                ..
            }
        }
    ));
    assert_eq!(
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        "second"
    );
    committed(second.join().unwrap());
    assert_eq!(gate.maximum.load(Ordering::SeqCst), 1);
}

#[test]
fn cancellation_while_waiting_is_observed_before_writer_admission() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let store = Arc::new(store);
    let expected_home = store.home_revision().unwrap();
    let gate = Arc::new(AssemblyGate::default());
    let (entered_tx, entered_rx) = mpsc::channel();

    let first_store = Arc::clone(&store);
    let first_gate = Arc::clone(&gate);
    let first = std::thread::spawn(move || {
        let mut command = HomeCommand::new(expected_home);
        command
            .add(alpha.contribution(
                first_store.domain_revision(&alpha).unwrap(),
                BlockingAssembly::<AlphaDomain>::new(first_gate, entered_tx, "first", 1, true),
            ))
            .unwrap();
        first_store.execute(command)
    });
    assert_eq!(
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        "first"
    );

    let cancellation = CommandCancellation::new();
    let queued_cancellation = cancellation.clone();
    let second_store = Arc::clone(&store);
    let (started_tx, started_rx) = mpsc::channel();
    let second = std::thread::spawn(move || {
        let mut command = HomeCommand::new(expected_home).with_cancellation(queued_cancellation);
        command
            .add(beta.contribution(
                second_store.domain_revision(&beta).unwrap(),
                PutBytes::<BetaDomain>::new(2, b"cancelled".to_vec()),
            ))
            .unwrap();
        started_tx.send(()).unwrap();
        second_store.execute(command)
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    cancellation.cancel();
    gate.release();

    assert!(matches!(
        first.join().unwrap(),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorAssembly { .. }
        }
    ));
    assert!(matches!(
        second.join().unwrap(),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::CancelledBeforeAdmission
        }
    ));
    assert_eq!(store.home_revision().unwrap(), expected_home);
}

fn seed(store: &beryl_home_store::HomeStore, alpha: &beryl_home_store::DomainHandle<AlphaDomain>) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"seed".to_vec()),
        ))
        .unwrap();
    committed(store.execute(command));
}
