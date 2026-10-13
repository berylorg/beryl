use std::{
    io::Write,
    num::NonZeroU64,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_home_store::{
    DomainHandle, HomeCommand, HomeDomainRequirements, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, PointReadLimit, SidecarAddress, SidecarByteLimit,
    SidecarNamespace, VerifiedSidecar, test_faults::FaultController,
};
use beryl_model::{DomainRevision, HomeRevision};

use super::support::{AlphaDomain, BytesRecord, PutBytes, committed};

static FIXTURE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static LEDGER_WRITE: Mutex<()> = Mutex::new(());

pub fn owned_directory() -> PathBuf {
    let ledger = PathBuf::from(
        std::env::var_os("BERYL_REPLACEMENT_FIXTURE_LEDGER")
            .expect("root must supply BERYL_REPLACEMENT_FIXTURE_LEDGER"),
    );
    assert!(ledger.is_absolute());
    let run_root = ledger.parent().unwrap().canonicalize().unwrap();
    let path = run_root.join(format!(
        "verified-sidecar-scope-{}-{}",
        std::process::id(),
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::SeqCst)
    ));
    let _lock = LEDGER_WRITE.lock().unwrap();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(ledger)
        .unwrap();
    writeln!(file, "{}", path.display()).unwrap();
    file.sync_all().unwrap();
    std::fs::create_dir(&path).unwrap();
    path
}

pub fn limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1024).unwrap())
}

pub fn open(path: &Path, faults: &FaultController) -> HomeStore {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    candidate.register_domain::<AlphaDomain>().unwrap();
    candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap()
}

pub fn seed(store: &HomeStore) -> SidecarAddress {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"verified scope bytes",
            limit(),
        )
        .unwrap();
    let address = sidecar.address().clone();
    let domain = store.domain_handle::<AlphaDomain>().unwrap();
    let revision = store.domain_revision(&domain).unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.require_sidecar(sidecar).unwrap();
    command
        .add(domain.contribution(
            revision,
            PutBytes::<AlphaDomain>::new(1, address.digest().as_bytes().to_vec()),
        ))
        .unwrap();
    committed(store.execute(command));
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(&domain).unwrap(),
            PutBytes::<AlphaDomain>::new(2, b"retained sidecar reference".to_vec()),
        ))
        .unwrap();
    committed(store.execute(command));
    address
}

pub fn verified(store: &HomeStore, address: &SidecarAddress) -> VerifiedSidecar {
    store.verify_sidecar(address, limit()).unwrap()
}

#[derive(Debug, PartialEq, Eq)]
pub struct PublishedState {
    home_revision: HomeRevision,
    domain_revision: DomainRevision,
    metadata: Option<Vec<u8>>,
    reference: Option<Vec<u8>>,
}

impl PublishedState {
    pub fn capture(store: &HomeStore) -> Self {
        let domain = store.domain_handle::<AlphaDomain>().unwrap();
        Self {
            home_revision: store.home_revision().unwrap(),
            domain_revision: store.domain_revision(&domain).unwrap(),
            metadata: record(store, &domain, 1),
            reference: record(store, &domain, 2),
        }
    }
}

fn record(store: &HomeStore, domain: &DomainHandle<AlphaDomain>, key: u64) -> Option<Vec<u8>> {
    store
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &key,
            PointReadLimit::new(1028).unwrap(),
        )
        .unwrap()
}
