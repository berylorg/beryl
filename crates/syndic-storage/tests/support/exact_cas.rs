#![allow(dead_code)]

use beryl_model::{
    CasConversationToolProfile, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath,
};

pub fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-test-root-history",
        )
        .unwrap(),
    )
}

pub fn tool_profile() -> CasConversationToolProfile {
    CasConversationToolProfile::v1([248; 32])
}
