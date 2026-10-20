use std::collections::HashMap;

use beryl_state::{InstalledThemeSelection, ThemeObservedDocument};
use gpui_settings_window::{SettingsPageSplitPageRequest, SettingsPageSplitRequestId};

use crate::{
    settings_service::SettingsDraftBinding,
    theme_runtime::{ThemeSettingsCatalog, ThemeSettingsTicket},
};

pub(super) mod commands;
mod model;
mod paging;

pub(super) enum ReadPurpose {
    Catalog {
        binding: Option<SettingsDraftBinding>,
    },
    Page {
        request: SettingsPageSplitPageRequest,
    },
    Document {
        binding: SettingsDraftBinding,
        selection: InstalledThemeSelection,
    },
    RefreshSelection {
        binding: SettingsDraftBinding,
        id: beryl_state::InstalledThemeId,
    },
    RefreshEditor {
        binding: SettingsDraftBinding,
        id: beryl_state::InstalledThemeId,
    },
    EditorDocument {
        binding: SettingsDraftBinding,
        selection: InstalledThemeSelection,
    },
}

#[derive(Default)]
pub(super) struct ThemesState {
    pub(super) catalog: Option<ThemeSettingsCatalog>,
    pub(super) epoch: u64,
    reads: HashMap<ThemeSettingsTicket, ReadPurpose>,
    pages: HashMap<SettingsPageSplitRequestId, Vec<InstalledThemeSelection>>,
    pub(super) selected: Option<InstalledThemeSelection>,
    selected_hint: Option<(String, usize)>,
    pub(super) document: Option<ThemeObservedDocument>,
    pub(super) editor: Option<super::theme_editor::ThemeDocumentEditor>,
    rename_name: String,
    pub(super) message: Option<&'static str>,
    pub(super) read_failed: bool,
    pub(super) repository: Option<commands::PendingRepository>,
    pub(super) repository_unavailable: bool,
}
