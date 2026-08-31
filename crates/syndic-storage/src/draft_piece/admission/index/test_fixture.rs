use super::*;

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionIndexTestErrorV1 {
    AssociationOutOfRange,
    DuplicateSource,
    DuplicateTarget,
    MissingNode,
    NodeIdOccupied,
    PathAuthentication,
    ProvenPageOwner,
    SourceTargetDisagreement,
    Schema(DraftMarkerAdmissionSchemaErrorV1),
}

#[cfg(feature = "test-faults")]
impl From<DraftMarkerAdmissionIndexPreparationErrorV1> for DraftMarkerAdmissionIndexTestErrorV1 {
    fn from(value: DraftMarkerAdmissionIndexPreparationErrorV1) -> Self {
        match value {
            DraftMarkerAdmissionIndexPreparationErrorV1::Read(_)
            | DraftMarkerAdmissionIndexPreparationErrorV1::StoreRead(_) => Self::PathAuthentication,
            DraftMarkerAdmissionIndexPreparationErrorV1::Schema(error) => Self::Schema(error),
            DraftMarkerAdmissionIndexPreparationErrorV1::AssociationOutOfRange => {
                Self::AssociationOutOfRange
            }
            DraftMarkerAdmissionIndexPreparationErrorV1::DuplicateSource => Self::DuplicateSource,
            DraftMarkerAdmissionIndexPreparationErrorV1::DuplicateTarget => Self::DuplicateTarget,
            DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode => Self::MissingNode,
            DraftMarkerAdmissionIndexPreparationErrorV1::NodeIdOccupied => Self::NodeIdOccupied,
            DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication => {
                Self::PathAuthentication
            }
            DraftMarkerAdmissionIndexPreparationErrorV1::ProvenPageOwner => Self::ProvenPageOwner,
            DraftMarkerAdmissionIndexPreparationErrorV1::SourceTargetDisagreement => {
                Self::SourceTargetDisagreement
            }
        }
    }
}

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct DraftMarkerAdmissionIndexTestStateV1 {
    owner: DraftMarkerAdmissionOwnerV1,
    source_root: DraftMarkerAdmissionRootV1,
    target_root: DraftMarkerAdmissionRootV1,
    nodes: Vec<DraftMarkerAdmissionNodeV1>,
    prior_replay_nodes: Vec<DraftMarkerAdmissionChildV1>,
    maximum_height: u8,
    command_limit: u64,
}

#[cfg(feature = "test-faults")]
pub struct DraftMarkerAdmissionIndexTestStepV1 {
    source_root: DraftMarkerAdmissionRootV1,
    target_root: DraftMarkerAdmissionRootV1,
    puts: Box<[DraftMarkerAdmissionNodeKeyV1]>,
    deletions: Box<[DraftMarkerAdmissionNodeKeyV1]>,
    retained_predecessor_nodes: Box<[DraftMarkerAdmissionNodeKeyV1]>,
    added: DraftMarkerAdmissionRetainedChargeV1,
    removed: DraftMarkerAdmissionRetainedChargeV1,
    footprint: DraftMarkerAdmissionIndexFootprintV1,
}

#[cfg(feature = "test-faults")]
impl DraftMarkerAdmissionIndexTestStepV1 {
    pub const fn source_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.source_root
    }

    pub const fn target_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.target_root
    }

    pub fn puts(&self) -> &[DraftMarkerAdmissionNodeKeyV1] {
        &self.puts
    }

    pub fn deletions(&self) -> &[DraftMarkerAdmissionNodeKeyV1] {
        &self.deletions
    }

    pub fn retained_predecessor_nodes(&self) -> &[DraftMarkerAdmissionNodeKeyV1] {
        &self.retained_predecessor_nodes
    }

    pub const fn added_charge(&self) -> DraftMarkerAdmissionRetainedChargeV1 {
        self.added
    }

    pub const fn removed_charge(&self) -> DraftMarkerAdmissionRetainedChargeV1 {
        self.removed
    }

    pub const fn read_bytes(&self) -> u64 {
        self.footprint.read_bytes
    }

    pub const fn write_bytes(&self) -> u64 {
        self.footprint.write_bytes
    }

    pub const fn delete_bytes(&self) -> u64 {
        self.footprint.delete_bytes
    }

    pub const fn command_bytes(&self) -> u64 {
        self.footprint.command_bytes
    }
}

#[cfg(feature = "test-faults")]
impl DraftMarkerAdmissionIndexTestStateV1 {
    pub fn new(owner: DraftMarkerAdmissionOwnerV1) -> Self {
        Self {
            owner,
            source_root: crate::canonical_empty_draft_marker_admission_root_v1(
                DraftMarkerAdmissionTreeV1::SourceOrder,
            ),
            target_root: crate::canonical_empty_draft_marker_admission_root_v1(
                DraftMarkerAdmissionTreeV1::TargetId,
            ),
            nodes: Vec::new(),
            prior_replay_nodes: Vec::new(),
            maximum_height: DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT,
            command_limit: crate::DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES,
        }
    }

    pub fn from_parts(
        owner: DraftMarkerAdmissionOwnerV1,
        source_root: DraftMarkerAdmissionRootV1,
        target_root: DraftMarkerAdmissionRootV1,
        nodes: impl Into<Vec<DraftMarkerAdmissionNodeV1>>,
        prior_replay_nodes: impl Into<Vec<DraftMarkerAdmissionChildV1>>,
    ) -> Self {
        Self {
            owner,
            source_root,
            target_root,
            nodes: nodes.into(),
            prior_replay_nodes: prior_replay_nodes.into(),
            maximum_height: DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT,
            command_limit: crate::DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES,
        }
    }

    pub const fn source_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.source_root
    }

    pub const fn target_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.target_root
    }

    pub fn prior_replay_nodes(&self) -> &[DraftMarkerAdmissionChildV1] {
        &self.prior_replay_nodes
    }

    pub fn nodes(&self) -> &[DraftMarkerAdmissionNodeV1] {
        &self.nodes
    }

    pub fn set_maximum_height_for_test(&mut self, maximum_height: u8) {
        self.maximum_height = maximum_height;
    }

    pub fn set_command_limit_for_test(&mut self, command_limit: u64) {
        self.command_limit = command_limit;
    }

    pub fn set_roots_for_test(
        &mut self,
        source_root: DraftMarkerAdmissionRootV1,
        target_root: DraftMarkerAdmissionRootV1,
    ) {
        self.source_root = source_root;
        self.target_root = target_root;
    }

    pub fn corrupt_source_root_height_for_test(&mut self, height: u8) {
        self.source_root = DraftMarkerAdmissionRootV1::from_parts(
            self.source_root.tree(),
            self.source_root.node(),
            height,
            self.source_root.digest(),
            self.source_root.count(),
        );
    }

    pub fn remove_node_for_test(&mut self, key: DraftMarkerAdmissionNodeKeyV1) -> bool {
        let Some(index) = self.nodes.iter().position(|node| node.key() == key) else {
            return false;
        };
        self.nodes.remove(index);
        true
    }

    pub fn corrupt_node_digest_for_test(&mut self, key: DraftMarkerAdmissionNodeKeyV1) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.key() == key) else {
            return false;
        };
        let mut digest = *node.digest().as_bytes();
        digest[0] ^= 0xff;
        *node = DraftMarkerAdmissionNodeV1::from_parts(
            node.key(),
            node.tree(),
            node.payload().clone(),
            crate::DraftMarkerAdmissionDigestV1::from_bytes(digest),
        );
        true
    }

    pub fn corrupt_first_child_count_for_test(
        &mut self,
        key: DraftMarkerAdmissionNodeKeyV1,
    ) -> bool {
        self.corrupt_first_child_for_test(key, false)
    }

    pub fn corrupt_first_child_envelope_for_test(
        &mut self,
        key: DraftMarkerAdmissionNodeKeyV1,
    ) -> bool {
        self.corrupt_first_child_for_test(key, true)
    }

    pub fn corrupt_node_fanout_for_test(&mut self, key: DraftMarkerAdmissionNodeKeyV1) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.key() == key) else {
            return false;
        };
        let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = node.payload()
        else {
            return false;
        };
        let Some(first) = children.first().copied() else {
            return false;
        };
        let mut children = children.to_vec();
        children.resize(DRAFT_MARKER_ADMISSION_TREE_FANOUT + 1, first);
        *node = DraftMarkerAdmissionNodeV1::from_parts(
            node.key(),
            node.tree(),
            DraftMarkerAdmissionNodePayloadV1::Internal {
                height: *height,
                children: children.into_boxed_slice(),
            },
            node.digest(),
        );
        true
    }

    pub fn prepare(
        &self,
        proven_page: &DraftMarkerLabelReadinessProvenPageV1,
        association_index: usize,
    ) -> Result<DraftMarkerAdmissionIndexTestStepV1, DraftMarkerAdmissionIndexTestErrorV1> {
        let reader = SliceAdmissionNodeReader { nodes: &self.nodes };
        let prepared = prepare_with_reader(
            &reader,
            self.owner,
            self.source_root,
            self.target_root,
            &self.prior_replay_nodes,
            proven_page,
            association_index,
            self.maximum_height,
            self.command_limit,
        )?;
        Ok(test_step(&prepared))
    }

    pub fn apply(
        &mut self,
        proven_page: &DraftMarkerLabelReadinessProvenPageV1,
        association_index: usize,
    ) -> Result<DraftMarkerAdmissionIndexTestStepV1, DraftMarkerAdmissionIndexTestErrorV1> {
        let reader = SliceAdmissionNodeReader { nodes: &self.nodes };
        let prepared = prepare_with_reader(
            &reader,
            self.owner,
            self.source_root,
            self.target_root,
            &self.prior_replay_nodes,
            proven_page,
            association_index,
            self.maximum_height,
            self.command_limit,
        )?;
        let mut nodes = self.nodes.clone();
        for deletion in prepared.deletions() {
            let index = nodes
                .iter()
                .position(|node| node.key() == deletion.key())
                .ok_or(DraftMarkerAdmissionIndexTestErrorV1::MissingNode)?;
            nodes.remove(index);
        }
        nodes.extend(prepared.puts().iter().cloned());
        let step = test_step(&prepared);
        self.source_root = prepared.source_root();
        self.target_root = prepared.target_root();
        self.prior_replay_nodes = prepared.retained_predecessor_nodes().to_vec();
        self.nodes = nodes;
        Ok(step)
    }

    fn corrupt_first_child_for_test(
        &mut self,
        key: DraftMarkerAdmissionNodeKeyV1,
        envelope: bool,
    ) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.key() == key) else {
            return false;
        };
        let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = node.payload()
        else {
            return false;
        };
        let mut children = children.to_vec();
        let Some(first) = children.first_mut() else {
            return false;
        };
        *first = if envelope {
            DraftMarkerAdmissionChildV1::new(
                first.key(),
                first.digest(),
                first.count(),
                match first.envelope() {
                    crate::DraftMarkerAdmissionEnvelopeV1::SourceOrder { .. } => {
                        crate::DraftMarkerAdmissionEnvelopeV1::TargetId {
                            first: SyndicDraftMarkerId::from_bytes([0; 16]),
                            last: SyndicDraftMarkerId::from_bytes([0; 16]),
                        }
                    }
                    crate::DraftMarkerAdmissionEnvelopeV1::TargetId { .. } => {
                        crate::DraftMarkerAdmissionEnvelopeV1::SourceOrder {
                            first: DraftMarkerAdmissionSourceKeyV1::new(
                                beryl_model::ImageLabelOrdinal::new(1)
                                    .expect("test label is nonzero"),
                                SyndicDraftMarkerId::from_bytes([0; 16]),
                            ),
                            last: DraftMarkerAdmissionSourceKeyV1::new(
                                beryl_model::ImageLabelOrdinal::new(1)
                                    .expect("test label is nonzero"),
                                SyndicDraftMarkerId::from_bytes([0; 16]),
                            ),
                        }
                    }
                },
            )
        } else {
            DraftMarkerAdmissionChildV1::new(
                first.key(),
                first.digest(),
                first.count().saturating_add(1),
                first.envelope(),
            )
        };
        *node = DraftMarkerAdmissionNodeV1::from_parts(
            node.key(),
            node.tree(),
            DraftMarkerAdmissionNodePayloadV1::Internal {
                height: *height,
                children: children.into_boxed_slice(),
            },
            node.digest(),
        );
        true
    }
}

#[cfg(feature = "test-faults")]
struct SliceAdmissionNodeReader<'a> {
    nodes: &'a [DraftMarkerAdmissionNodeV1],
}

#[cfg(feature = "test-faults")]
impl AdmissionNodeReader for SliceAdmissionNodeReader<'_> {
    fn point(
        &self,
        key: &DraftMarkerAdmissionNodeKeyV1,
    ) -> Result<Option<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionIndexPreparationErrorV1>
    {
        Ok(self.nodes.iter().find(|node| node.key() == *key).cloned())
    }
}

#[cfg(feature = "test-faults")]
fn test_step(
    prepared: &PreparedDraftMarkerAdmissionIndexSuccessorV1,
) -> DraftMarkerAdmissionIndexTestStepV1 {
    let delta = prepared.retained_charge_delta();
    DraftMarkerAdmissionIndexTestStepV1 {
        source_root: prepared.source_root(),
        target_root: prepared.target_root(),
        puts: prepared
            .puts()
            .iter()
            .map(DraftMarkerAdmissionNodeV1::key)
            .collect(),
        deletions: prepared
            .deletions()
            .iter()
            .map(DraftMarkerAdmissionNodeV1::key)
            .collect(),
        retained_predecessor_nodes: prepared
            .retained_predecessor_nodes()
            .iter()
            .map(|child| child.key())
            .collect(),
        added: delta.added,
        removed: delta.removed,
        footprint: prepared.footprint(),
    }
}
