fn phase79_lineage() -> CasLineageProof {
    CasLineageProof::native(
        NativeCasLineage::Fresh,
        CasRepresentedPrefixProof::new(
            None,
            ThreadRevision::new(1).unwrap(),
            empty_selected_path_digest(),
        ),
    )
    .unwrap()
}

fn phase79_execution_binding(runtime_id: RuntimeId, root: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([root; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl",
        )
        .unwrap(),
    )
}

fn phase79_register_candidate_lease(
    service: &ProjectionConnectionService,
    connection: &Arc<ProjectionConnection>,
    cas_thread_id: CasThreadId,
    owner: SyndicThreadId,
) -> LoadedProjectionLease {
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let lease = connection
        .register_new(cas_thread_id, owner, Duration::from_secs(10), Some(&issuer))
        .unwrap();
    drop(worker);
    lease
}

fn phase79_acquire_candidate_sibling(
    service: &ProjectionConnectionService,
    connection: &Arc<ProjectionConnection>,
    cas_thread_id: &CasThreadId,
    owner: SyndicThreadId,
) -> LoadedProjectionLease {
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let lease = match connection
        .acquire_existing(cas_thread_id, owner, Duration::from_secs(10), Some(&issuer))
        .unwrap()
    {
        crate::cas_projection::connection::ExistingLease::Exact(lease) => lease,
        _ => panic!("the exact loaded projection must mint one sibling lease"),
    };
    drop(worker);
    lease
}


include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/pending_projection_quarantine/siblings.rs"));
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/pending_projection_quarantine/failures.rs"));
