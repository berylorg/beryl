use std::path::Path;

use fjall::{
    BlockReadLimits, Config, DatabaseStorageLimits, RetainedTopologyLimits,
    SeparatedValueReadLimits, StoragePolicy, TreeReadLimits,
};

const MIB_U32: u32 = 1_024 * 1_024;
const MIB_U64: u64 = 1_024 * 1_024;
const RECONCILIATION_DESCRIPTOR_BYTES: usize = 64 * 1_024 * 1_024;
const RECONCILIATION_RESERVED_BYTES: usize = 256 * 1_024 * 1_024;

/// The one practical V1 storage profile retained across same-home generations.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StorageProfile {
    policy: StoragePolicy,
    reconciliation_descriptor_bytes: usize,
    reconciliation_reserved_bytes: usize,
}

impl StorageProfile {
    pub(crate) fn production() -> fjall::Result<Self> {
        let blocks = BlockReadLimits::new(MIB_U32, 4 * MIB_U32)?;
        let separated_values = SeparatedValueReadLimits::new(MIB_U32, MIB_U32)?;
        let topology = RetainedTopologyLimits::new(4_096, 4_096, 8_192, 1_024)?;
        let tree =
            TreeReadLimits::new(u32::from(u16::MAX), blocks, separated_values, 64, topology)?;
        let database = DatabaseStorageLimits::new(
            1_024,
            u64::from(u8::MAX),
            64,
            16_384,
            32 * MIB_U64,
            64 * MIB_U64,
            2 * MIB_U64,
            2 * MIB_U64,
            256 * MIB_U64,
            256 * MIB_U64,
            1_000_000,
        )?;
        Ok(Self {
            policy: StoragePolicy::new(tree, database)?,
            reconciliation_descriptor_bytes: RECONCILIATION_DESCRIPTOR_BYTES,
            reconciliation_reserved_bytes: RECONCILIATION_RESERVED_BYTES,
        })
    }

    /// Constructs a single-use configuration with a fresh cache and memtable owner.
    pub(crate) fn configuration(self, path: &Path) -> fjall::Result<Config> {
        Config::new(self.policy, path)
    }

    pub(crate) const fn reconciliation_descriptor_bytes(self) -> usize {
        self.reconciliation_descriptor_bytes
    }

    pub(crate) const fn reconciliation_reserved_bytes(self) -> usize {
        self.reconciliation_reserved_bytes
    }
}
