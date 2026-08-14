#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RoleDefaults {
    pub(super) background: &'static str,
    pub(super) border: &'static str,
    pub(super) foreground: &'static str,
    pub(super) text_background: &'static str,
    pub(super) font_family: &'static str,
    pub(super) font_size: f32,
    pub(super) font_weight: u16,
}

pub(super) const APP: RoleDefaults = role_defaults_value(
    "#0b1020", "#243047", "#e7eef7", "#0b1020", "Inter", 14.0, 400,
);
pub(super) const PANEL: RoleDefaults = role_defaults_value(
    "#111827", "#2f3b52", "#e7eef7", "#111827", "Inter", 14.0, 400,
);
pub(super) const POPUP: RoleDefaults = role_defaults_value(
    "#101827", "#3a4860", "#f3f7fb", "#101827", "Inter", 14.0, 400,
);
pub(super) const ROW: RoleDefaults = role_defaults_value(
    "#172033", "#2d3a52", "#e7eef7", "#172033", "Inter", 14.0, 400,
);
pub(super) const ROW_HOVER: RoleDefaults = role_defaults_value(
    "#1f2b42", "#40516b", "#f3f7fb", "#1f2b42", "Inter", 14.0, 400,
);
pub(super) const SELECTED: RoleDefaults = role_defaults_value(
    "#173a5e", "#38bdf8", "#ffffff", "#173a5e", "Inter", 14.0, 500,
);
pub(super) const DISABLED: RoleDefaults = role_defaults_value(
    "#141b2a", "#2b3547", "#7f8ea3", "#141b2a", "Inter", 14.0, 400,
);
pub(super) const MUTED: RoleDefaults = role_defaults_value(
    "#172033", "#2d3a52", "#aab7c9", "#172033", "Inter", 14.0, 400,
);
pub(super) const PENDING: RoleDefaults = role_defaults_value(
    "#1d2d3d", "#60a5fa", "#bfdbfe", "#1d2d3d", "Inter", 14.0, 500,
);
pub(super) const UNAVAILABLE: RoleDefaults = role_defaults_value(
    "#211b2b", "#65536f", "#c8b7d8", "#211b2b", "Inter", 14.0, 400,
);
pub(super) const WARNING: RoleDefaults = role_defaults_value(
    "#34260f", "#f59e0b", "#fde68a", "#34260f", "Inter", 14.0, 500,
);
pub(super) const ERROR: RoleDefaults = role_defaults_value(
    "#3a171c", "#ef4444", "#fecaca", "#3a171c", "Inter", 14.0, 500,
);
pub(super) const INFO: RoleDefaults = role_defaults_value(
    "#12283b", "#38bdf8", "#bae6fd", "#12283b", "Inter", 14.0, 500,
);
pub(super) const SUCCESS: RoleDefaults = role_defaults_value(
    "#103022", "#22c55e", "#bbf7d0", "#103022", "Inter", 14.0, 500,
);
pub(super) const ACCENT: RoleDefaults = role_defaults_value(
    "#103247", "#22d3ee", "#cffafe", "#103247", "Inter", 14.0, 500,
);
pub(super) const SEPARATOR: RoleDefaults = role_defaults_value(
    "#0b1020", "#334155", "#94a3b8", "#0b1020", "Inter", 14.0, 400,
);
pub(super) const OVERLAY: RoleDefaults = role_defaults_value(
    "#050816", "#050816", "#cbd5e1", "#050816", "Inter", 14.0, 400,
);
pub(super) const PRIMARY_BUTTON: RoleDefaults = role_defaults_value(
    "#1d4ed8", "#3b82f6", "#eff6ff", "#1d4ed8", "Inter", 14.0, 500,
);
pub(super) const PRIMARY_BUTTON_HOVER: RoleDefaults = role_defaults_value(
    "#2563eb", "#60a5fa", "#ffffff", "#2563eb", "Inter", 14.0, 500,
);
pub(super) const PRIMARY_BUTTON_PRESSED: RoleDefaults = role_defaults_value(
    "#1e40af", "#38bdf8", "#ffffff", "#1e40af", "Inter", 14.0, 500,
);
pub(super) const PRIMARY_BUTTON_ACTIVE: RoleDefaults = role_defaults_value(
    "#164e63", "#22d3ee", "#ecfeff", "#164e63", "Inter", 14.0, 500,
);
pub(super) const SECONDARY_BUTTON: RoleDefaults = role_defaults_value(
    "#1e293b", "#475569", "#e7eef7", "#1e293b", "Inter", 14.0, 500,
);
pub(super) const SECONDARY_BUTTON_HOVER: RoleDefaults = role_defaults_value(
    "#2b3850", "#64748b", "#f8fafc", "#2b3850", "Inter", 14.0, 500,
);
pub(super) const SECONDARY_BUTTON_PRESSED: RoleDefaults = role_defaults_value(
    "#111827", "#38bdf8", "#f8fafc", "#111827", "Inter", 14.0, 500,
);
pub(super) const SECONDARY_BUTTON_ACTIVE: RoleDefaults = role_defaults_value(
    "#173a5e", "#38bdf8", "#ffffff", "#173a5e", "Inter", 14.0, 500,
);
pub(super) const BUTTON_DISABLED: RoleDefaults = role_defaults_value(
    "#1a2233", "#334155", "#8b98aa", "#1a2233", "Inter", 14.0, 500,
);
pub(super) const INPUT: RoleDefaults = role_defaults_value(
    "#0f172a", "#334155", "#e7eef7", "#0f172a", "Inter", 14.0, 400,
);
pub(super) const INPUT_FOCUSED: RoleDefaults = role_defaults_value(
    "#111d2e", "#38bdf8", "#f8fafc", "#111d2e", "Inter", 14.0, 400,
);
pub(super) const TRANSCRIPT: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#e7eef7", "#091220", "Inter", 14.0, 400,
);
pub(super) const TRANSCRIPT_TEXT: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#e2e8f0", "#091220", "Inter", 14.0, 400,
);
pub(super) const COMMENTARY: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#cbd5e1", "#091220", "Inter", 14.0, 400,
);
pub(super) const REASONING: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#d7dee8", "#091220", "Inter", 14.0, 400,
);
pub(super) const USER_INPUT: RoleDefaults = role_defaults_value(
    "#0d1c2d", "#1f4f6f", "#f8fafc", "#0d1c2d", "Inter", 14.0, 400,
);
pub(super) const HEADING: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#93c5fd", "#091220", "Inter", 18.0, 600,
);
pub(super) const EMPHASIS: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#bfdbfe", "#091220", "Inter", 14.0, 400,
);
pub(super) const STRONG_EMPHASIS: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#f8fafc", "#091220", "Inter", 14.0, 700,
);
pub(super) const LINK: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#7dd3fc", "#091220", "Inter", 14.0, 500,
);
pub(super) const BLOCK_QUOTE: RoleDefaults = role_defaults_value(
    "#101827", "#3b4a60", "#d8e1ee", "#101827", "Inter", 14.0, 400,
);
pub(super) const LIST_MARKER: RoleDefaults = role_defaults_value(
    "#091220", "#1f2937", "#a5b4fc", "#091220", "Inter", 14.0, 500,
);
pub(super) const INLINE_CODE: RoleDefaults = role_defaults_value(
    "#0f172a", "#334155", "#e2e8f0", "#0f172a", "Consolas", 13.0, 400,
);
pub(super) const CODE: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#e2e8f0", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const CODE_PANEL: RoleDefaults = role_defaults_value(
    "#0b1220", "#1f2937", "#e2e8f0", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const CODE_PANEL_HEADER: RoleDefaults = role_defaults_value(
    "#111827", "#334155", "#cbd5e1", "#111827", "Inter", 13.0, 500,
);
pub(super) const CODE_PANEL_BORDER: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#e2e8f0", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const STATUS_LINE: RoleDefaults = role_defaults_value(
    "#050816", "#1e293b", "#e2e8f0", "#050816", "Inter", 12.0, 400,
);
pub(super) const WORKING: RoleDefaults = role_defaults_value(
    "#092233", "#38bdf8", "#bae6fd", "#092233", "Inter", 12.0, 500,
);
pub(super) const COMPACTING: RoleDefaults = role_defaults_value(
    "#211b3c", "#a78bfa", "#ddd6fe", "#211b3c", "Inter", 12.0, 500,
);
pub(super) const STREAMING: RoleDefaults = role_defaults_value(
    "#0d2b27", "#2dd4bf", "#ccfbf1", "#0d2b27", "Inter", 12.0, 500,
);
pub(super) const SCROLLBAR: RoleDefaults = role_defaults_value(
    "#334155", "#334155", "#cbd5e1", "#334155", "Inter", 14.0, 400,
);
pub(super) const SCROLLBAR_HOVER: RoleDefaults = role_defaults_value(
    "#64748b", "#64748b", "#f8fafc", "#64748b", "Inter", 14.0, 400,
);
pub(super) const MEDIA: RoleDefaults = role_defaults_value(
    "#111827", "#334155", "#cbd5e1", "#111827", "Inter", 13.0, 400,
);
pub(super) const MEDIA_BORDER: RoleDefaults = role_defaults_value(
    "#111827", "#475569", "#cbd5e1", "#111827", "Inter", 13.0, 400,
);
pub(super) const SYNTAX_HEADING: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#93c5fd", "#0b1220", "Consolas", 13.0, 600,
);
pub(super) const SYNTAX_QUOTE: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#a7f3d0", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_LIST: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#c4b5fd", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_PUNCTUATION: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#94a3b8", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_KEY: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#7dd3fc", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_STRING: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#86efac", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_NUMBER: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#fca5a5", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_BOOLEAN: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#f0abfc", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_NULL: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#cbd5e1", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_DATE: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#fcd34d", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_COMMENT: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#94a3b8", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_ASSIGNMENT: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#67e8f9", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_ESCAPE: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#f9a8d4", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_IMAGE: RoleDefaults = role_defaults_value(
    "#0b1220", "#334155", "#fdba74", "#0b1220", "Consolas", 13.0, 400,
);
pub(super) const SYNTAX_ERROR: RoleDefaults = role_defaults_value(
    "#3a171c", "#ef4444", "#fecaca", "#3a171c", "Consolas", 13.0, 500,
);

const fn role_defaults_value(
    background: &'static str,
    border: &'static str,
    foreground: &'static str,
    text_background: &'static str,
    font_family: &'static str,
    font_size: f32,
    font_weight: u16,
) -> RoleDefaults {
    RoleDefaults {
        background,
        border,
        foreground,
        text_background,
        font_family,
        font_size,
        font_weight,
    }
}
