use std::{
    error::Error,
    fmt,
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, DomainCallbackError, DomainCallbackSource,
    DomainMutation, DomainReader, DomainReconciliation, DomainSchemaVersion, HomeCommand,
    HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuildError,
    MutationBuilder, ReadError, ReadStage, ReconciliationReader, ReconciliationReservation,
    ReconciliationResolution, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
    SuccessorCorrelation, SuccessorObservation, SuccessorPointRead, SuccessorPointReader,
    SuccessorPointRecord, SuccessorProtocol, SuccessorReadReservation, SuccessorSource,
    SuccessorWitness,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

static SOURCE_CALLS: AtomicUsize = AtomicUsize::new(0);
static BLOCK_SOURCE: AtomicBool = AtomicBool::new(false);
static RELEASE_SOURCE: AtomicBool = AtomicBool::new(false);
static FAIL_SOURCE: AtomicBool = AtomicBool::new(false);
static OVERSIZED_EXPECTED_REJECTIONS: AtomicUsize = AtomicUsize::new(0);
static DERIVED_CURRENT_DECODE_CALLS: AtomicUsize = AtomicUsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

const INVALID_DERIVED_KEY: u64 = u64::MAX;
const OVERSIZED_DERIVED_KEY: u64 = u64::MAX - 1;

include!("domain.rs");
include!("protocol.rs");
include!("mutations.rs");
include!("harness.rs");

mod flight_capacity;
mod ordinary_source;
mod witness_adversarial;
