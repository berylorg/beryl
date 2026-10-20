mod appearance;
mod controller;
mod fields;
mod presentation;
pub mod theme_editor;
mod themes;

pub use controller::{SettingsWindowCommand, SettingsWindowController, SettingsWindowMountError};

pub const DEVELOPER_INSTRUCTIONS_DRAFT_LIMIT: usize = 64 * 1024;
