use super::super::DraftMarkerAdmissionCommandIdV1;
use super::*;
use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits};

pub struct DraftMarkerAdmissionPublicationFixtureV1 {
    seed: DraftMarkerAdmissionPublicationSeedV1,
    command_limit: u64,
}

pub struct DraftMarkerAdmissionPublicationSnapshotV1 {
    capacity: Option<DraftMarkerAdmissionCapacityV1>,
    head: Option<DraftMarkerAdmissionHeadV1>,
    receipt: Option<DraftMarkerAdmissionReplayReceiptV1>,
    nodes: Box<[Option<super::super::DraftMarkerAdmissionNodeV1>]>,
}

impl DraftMarkerAdmissionPublicationSnapshotV1 {
    pub fn capacity(&self) -> Option<&DraftMarkerAdmissionCapacityV1> {
        self.capacity.as_ref()
    }

    pub fn head(&self) -> Option<&DraftMarkerAdmissionHeadV1> {
        self.head.as_ref()
    }

    pub fn receipt(&self) -> Option<&DraftMarkerAdmissionReplayReceiptV1> {
        self.receipt.as_ref()
    }

    pub fn nodes(&self) -> &[Option<super::super::DraftMarkerAdmissionNodeV1>] {
        &self.nodes
    }
}

impl DraftMarkerAdmissionPublicationFixtureV1 {
    pub fn new(
        owner: DraftMarkerAdmissionOwnerV1,
        home_generation: NonZeroU64,
        request_commitment: DraftMarkerAdmissionDigestV1,
        custody_commitment: DraftMarkerAdmissionDigestV1,
        occurrence_commitment: DraftMarkerAdmissionDigestV1,
        source_head_bytes: impl Into<Box<[u8]>>,
        target_head_bytes: impl Into<Box<[u8]>>,
    ) -> Self {
        Self {
            seed: DraftMarkerAdmissionPublicationSeedV1::new(
                owner,
                home_generation,
                request_commitment,
                custody_commitment,
                occurrence_commitment,
                source_head_bytes,
                target_head_bytes,
            ),
            command_limit: DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES,
        }
    }

    pub fn with_command_limit_for_test(mut self, command_limit: u64) -> Self {
        self.command_limit = command_limit;
        self
    }

    pub fn limits_accept_for_test(
        operation: DraftMarkerAdmissionRetainedChargeV1,
        aggregate: DraftMarkerAdmissionRetainedChargeV1,
    ) -> bool {
        enforce_limits(operation, aggregate).is_ok()
    }

    pub fn current_command(
        self,
        storage: &SyndicStorage,
        page: DraftMarkerLabelReadinessProvenPageV1,
    ) -> CurrentDomainCommand {
        if self.command_limit == DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES {
            return storage.current_publish_draft_marker_admission_v1(self.seed, page);
        }
        storage.handle.current_command(PublicationMutation {
            seed: self.seed,
            page,
            command_limit: self.command_limit,
        })
    }
}

impl SyndicStorage {
    pub fn draft_marker_admission_receipt_for_test(
        &self,
        store: &beryl_home_store::HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: super::super::DraftMarkerAdmissionCommandIdV1,
    ) -> Result<Option<DraftMarkerAdmissionReplayReceiptV1>, ReadError> {
        store.read_point::<SyndicDomain, DraftMarkerAdmissionReceiptsCodec>(
            &self.handle,
            &DraftMarkerAdmissionReceiptKeyV1::new(owner, command),
            family_point_limit::<DraftMarkerAdmissionReceiptsFamily>(),
        )
    }

    pub fn draft_marker_admission_publication_snapshot_for_test(
        &self,
        store: &beryl_home_store::HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        node_keys: &[super::super::DraftMarkerAdmissionNodeKeyV1],
    ) -> Result<DraftMarkerAdmissionPublicationSnapshotV1, ReadError> {
        let capacity = store.read_point::<SyndicDomain, DraftMarkerAdmissionCapacityCodec>(
            &self.handle,
            &DraftMarkerAdmissionCapacityKeyV1,
            family_point_limit::<DraftMarkerAdmissionCapacityFamily>(),
        )?;
        let head = store.read_point::<SyndicDomain, DraftMarkerAdmissionHeadsCodec>(
            &self.handle,
            &owner,
            family_point_limit::<DraftMarkerAdmissionHeadsFamily>(),
        )?;
        let receipt = match head.as_ref().and_then(|head| head.selected_receipt()) {
            Some(command) => store.read_point::<SyndicDomain, DraftMarkerAdmissionReceiptsCodec>(
                &self.handle,
                &DraftMarkerAdmissionReceiptKeyV1::new(owner, command),
                family_point_limit::<DraftMarkerAdmissionReceiptsFamily>(),
            )?,
            None if head.as_ref().is_some_and(|head| {
                head.lifecycle() == DraftMarkerAdmissionLifecycleV1::TerminalCleanup
            }) =>
            {
                let page = store.read_cursor::<SyndicDomain, DraftMarkerAdmissionReceiptsCodec>(
                    &self.handle,
                    &CursorRange::closed(
                        DraftMarkerAdmissionReceiptKeyV1::new(
                            owner,
                            DraftMarkerAdmissionCommandIdV1::from_bytes([0; 16]),
                        ),
                        DraftMarkerAdmissionReceiptKeyV1::new(
                            owner,
                            DraftMarkerAdmissionCommandIdV1::from_bytes([u8::MAX; 16]),
                        ),
                    ),
                    CursorDirection::Forward,
                    CursorReadLimits::new(2, 512 * 1024)
                        .expect("terminal receipt fixture limits are nonzero"),
                )?;
                if page.records().len() == 1 && !page.has_more() {
                    Some(page.records()[0].value().clone())
                } else {
                    None
                }
            }
            None => None,
        };
        let mut nodes = Vec::with_capacity(node_keys.len());
        for key in node_keys {
            nodes.push(
                store.read_point::<SyndicDomain, DraftMarkerAdmissionNodesCodec>(
                    &self.handle,
                    key,
                    family_point_limit::<super::super::DraftMarkerAdmissionNodesFamily>(),
                )?,
            );
        }
        Ok(DraftMarkerAdmissionPublicationSnapshotV1 {
            capacity,
            head,
            receipt,
            nodes: nodes.into_boxed_slice(),
        })
    }
}
