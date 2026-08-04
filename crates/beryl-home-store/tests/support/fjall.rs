use std::path::Path;

use fjall::{
    BlockReadLimits, Config, DatabaseStorageLimits, RetainedTopologyLimits,
    SeparatedValueReadLimits, StoragePolicy, TreeReadLimits,
};

pub fn config(path: &Path) -> Config {
    Config::new(storage_policy(), path).expect("fixture Fjall configuration should be valid")
}

fn storage_policy() -> StoragePolicy {
    let block = BlockReadLimits::new(1024 * 1024, 4 * 1024 * 1024)
        .expect("fixture block limits should be valid");
    let separated = SeparatedValueReadLimits::new(1024 * 1024, 1024 * 1024)
        .expect("fixture separated-value limits should be valid");
    let topology = RetainedTopologyLimits::new(4_096, 4_096, 8_192, 1_024)
        .expect("fixture topology limits should be valid");
    let tree = TreeReadLimits::new(u16::MAX.into(), block, separated, 64, topology)
        .expect("fixture tree limits should be valid");
    let database = DatabaseStorageLimits::new(
        1_024,
        255,
        64,
        16_384,
        32 * 1024 * 1024,
        64 * 1024 * 1024,
        2 * 1024 * 1024,
        2 * 1024 * 1024,
        256 * 1024 * 1024,
        256 * 1024 * 1024,
        1_000_000,
    )
    .expect("fixture database limits should be valid");
    StoragePolicy::new(tree, database).expect("fixture storage policy should be valid")
}
