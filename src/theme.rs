use gpui::Rgba;

/// Zed One Dark Theme - Color Constants
/// Based on Zed's official One Dark theme specification
pub struct OneDarkTheme;

impl OneDarkTheme {
    // === BACKGROUND COLORS ===

    /// Main application background (#3b414d)
    pub fn background() -> Rgba {
        gpui::rgb(0x3b414d)
    }

    /// Surface background for panels and panes (#2f343e)
    pub fn surface_background() -> Rgba {
        gpui::rgb(0x2f343e)
    }

    /// Elevated surface background (#2f343e)
    pub fn elevated_surface_background() -> Rgba {
        gpui::rgb(0x2f343e)
    }

    /// Editor/content area background (#282c33)
    pub fn editor_background() -> Rgba {
        gpui::rgb(0x282c33)
    }

    /// UI element default background (#2e343e)
    pub fn element_background() -> Rgba {
        gpui::rgb(0x2e343e)
    }

    /// Hovered element background (#363c46)
    pub fn element_hover() -> Rgba {
        gpui::rgb(0x363c46)
    }

    /// Active/pressed element background (#454a56)
    pub fn element_active() -> Rgba {
        gpui::rgb(0x454a56)
    }

    /// Selected element background (#454a56)
    pub fn element_selected() -> Rgba {
        gpui::rgb(0x454a56)
    }

    /// Disabled element background (#2e343e)
    pub fn element_disabled() -> Rgba {
        gpui::rgb(0x2e343e)
    }

    // === TEXT COLORS ===

    /// Primary text color (#dce0e5)
    pub fn text() -> Rgba {
        gpui::rgb(0xdce0e5)
    }

    /// De-emphasized/muted text (#a9afbc)
    pub fn text_muted() -> Rgba {
        gpui::rgb(0xa9afbc)
    }

    /// Placeholder text (#878a98)
    pub fn text_placeholder() -> Rgba {
        gpui::rgb(0x878a98)
    }

    /// Disabled text (#878a98)
    pub fn text_disabled() -> Rgba {
        gpui::rgb(0x878a98)
    }

    /// Accent text for links and highlights (#74ade8)
    pub fn text_accent() -> Rgba {
        gpui::rgb(0x74ade8)
    }

    // === BORDER COLORS ===

    /// Standard border color (#464b57)
    pub fn border() -> Rgba {
        gpui::rgb(0x464b57)
    }

    /// Variant border color (#363c46)
    pub fn border_variant() -> Rgba {
        gpui::rgb(0x363c46)
    }

    /// Focused border color (#47679e)
    pub fn border_focused() -> Rgba {
        gpui::rgb(0x47679e)
    }

    /// Selected border color (#293b5b)
    pub fn border_selected() -> Rgba {
        gpui::rgb(0x293b5b)
    }

    /// Disabled border color (#414754)
    pub fn border_disabled() -> Rgba {
        gpui::rgb(0x414754)
    }

    /// Transparent border
    pub fn border_transparent() -> Rgba {
        gpui::rgba(0x00000000)
    }

    // === SEMANTIC COLORS ===

    /// Success/positive action color (#a1c181)
    pub fn success() -> Rgba {
        gpui::rgb(0xa1c181)
    }

    /// Error/danger color (#d07277)
    pub fn error() -> Rgba {
        gpui::rgb(0xd07277)
    }

    /// Warning color (#dec184)
    pub fn warning() -> Rgba {
        gpui::rgb(0xdec184)
    }

    /// Info/primary action color (#74ade8)
    pub fn info() -> Rgba {
        gpui::rgb(0x74ade8)
    }

    // === SYNTAX COLORS (for code/text highlighting) ===

    /// Comment color (#5d636f)
    pub fn syntax_comment() -> Rgba {
        gpui::rgb(0x5d636f)
    }

    /// String color (#a1c181)
    pub fn syntax_string() -> Rgba {
        gpui::rgb(0xa1c181)
    }

    /// Number color (#bf956a)
    pub fn syntax_number() -> Rgba {
        gpui::rgb(0xbf956a)
    }

    /// Keyword color (#b477cf)
    pub fn syntax_keyword() -> Rgba {
        gpui::rgb(0xb477cf)
    }

    /// Function color (#73ade9)
    pub fn syntax_function() -> Rgba {
        gpui::rgb(0x73ade9)
    }

    // === ICON COLORS ===

    /// Default icon color (matches primary text)
    pub fn icon() -> Rgba {
        Self::text()
    }

    /// Muted icon color
    pub fn icon_muted() -> Rgba {
        Self::text_muted()
    }

    /// Disabled icon color
    pub fn icon_disabled() -> Rgba {
        Self::text_disabled()
    }

    // === EDITOR SPECIFIC ===

    /// Editor foreground text (#acb2be)
    pub fn editor_foreground() -> Rgba {
        gpui::rgb(0xacb2be)
    }

    /// Editor line number (#4e5a5f)
    pub fn editor_line_number() -> Rgba {
        gpui::rgb(0x4e5a5f)
    }

    /// Active line number (#d0d4da)
    pub fn editor_active_line_number() -> Rgba {
        gpui::rgb(0xd0d4da)
    }

    // === GHOST ELEMENT COLORS (transparent backgrounds) ===

    /// Ghost element hover state
    pub fn ghost_element_hover() -> Rgba {
        gpui::rgb(0x363c46)
    }

    /// Ghost element active state
    pub fn ghost_element_active() -> Rgba {
        gpui::rgb(0x454a56)
    }

    /// Ghost element selected state
    pub fn ghost_element_selected() -> Rgba {
        gpui::rgb(0x454a56)
    }
}

/// Typography constants based on Zed's text sizing system
/// Base rem size: 16px
pub struct Typography;

impl Typography {
    // === UI TEXT SIZES ===

    /// Large UI text: 16px (1.0rem)
    pub const TEXT_UI_LG: f32 = 16.0;

    /// Default UI text: 14px (0.875rem)
    pub const TEXT_UI: f32 = 14.0;

    /// Small UI text: 12px (0.75rem)
    pub const TEXT_UI_SM: f32 = 12.0;

    /// Extra small UI text: 10px (0.625rem)
    pub const TEXT_UI_XS: f32 = 10.0;

    // === HEADLINE SIZES ===

    /// Extra small headline: 14px
    pub const HEADLINE_XS: f32 = 14.0;

    /// Small headline: 16px
    pub const HEADLINE_SM: f32 = 16.0;

    /// Medium headline: 18px
    pub const HEADLINE_MD: f32 = 18.0;

    /// Large headline: 20px
    pub const HEADLINE_LG: f32 = 20.0;

    /// Extra large headline: 23px
    pub const HEADLINE_XL: f32 = 23.0;

    // === LINE HEIGHTS ===

    /// Standard line height for headlines
    pub const LINE_HEIGHT_HEADLINE: f32 = 25.6;

    /// Tight line height for UI text
    pub const LINE_HEIGHT_TIGHT: f32 = 1.2;

    /// Normal line height
    pub const LINE_HEIGHT_NORMAL: f32 = 1.5;

    /// Relaxed line height
    pub const LINE_HEIGHT_RELAXED: f32 = 1.6;
}

/// Spacing constants based on Zed's DynamicSpacing system
/// Values are for "Default" density (Compact and Comfortable have different values)
pub struct Spacing;

impl Spacing {
    /// 0px - No spacing
    pub const BASE_00: f32 = 0.0;

    /// 1px - Minimal spacing
    pub const BASE_01: f32 = 1.0;

    /// 2px - Extra small spacing
    pub const BASE_02: f32 = 2.0;

    /// 3px - Tiny spacing
    pub const BASE_03: f32 = 3.0;

    /// 4px - Small spacing
    pub const BASE_04: f32 = 4.0;

    /// 6px - Medium-small spacing
    pub const BASE_06: f32 = 6.0;

    /// 8px - Medium spacing
    pub const BASE_08: f32 = 8.0;

    /// 12px - Large spacing
    pub const BASE_12: f32 = 12.0;

    /// 16px - Extra large spacing
    pub const BASE_16: f32 = 16.0;

    /// 20px - Double extra large spacing
    pub const BASE_20: f32 = 20.0;

    /// 24px - Triple extra large spacing
    pub const BASE_24: f32 = 24.0;

    /// 32px - Huge spacing
    pub const BASE_32: f32 = 32.0;

    /// 40px - Extra huge spacing
    pub const BASE_40: f32 = 40.0;

    /// 48px - Massive spacing
    pub const BASE_48: f32 = 48.0;
}

/// Button sizing constants
pub struct ButtonSize;

impl ButtonSize {
    /// Large button height: 32px
    pub const LARGE_HEIGHT: f32 = 32.0;
    pub const LARGE_PADDING: f32 = Spacing::BASE_08;

    /// Medium button height: 28px
    pub const MEDIUM_HEIGHT: f32 = 28.0;
    pub const MEDIUM_PADDING: f32 = Spacing::BASE_08;

    /// Default button height: 22px
    pub const DEFAULT_HEIGHT: f32 = 22.0;
    pub const DEFAULT_PADDING: f32 = Spacing::BASE_04;

    /// Compact button height: 18px
    pub const COMPACT_HEIGHT: f32 = 18.0;
    pub const COMPACT_PADDING: f32 = Spacing::BASE_04;

    /// Minimal button height: 16px
    pub const MINIMAL_HEIGHT: f32 = 16.0;
    pub const MINIMAL_PADDING: f32 = Spacing::BASE_00;
}

/// Checkbox/toggle sizing constants
pub struct CheckboxSize;

impl CheckboxSize {
    /// Container size: 20x20px
    pub const CONTAINER: f32 = 20.0;

    /// Inner box size: 4x4px
    pub const INNER_BOX: f32 = 4.0;

    /// Border width: 1px
    pub const BORDER_WIDTH: f32 = 1.0;

    /// Border radius (small)
    pub const BORDER_RADIUS: f32 = 2.0;
}

/// Input field sizing constants
pub struct InputSize;

impl InputSize {
    /// Default input height: 30px (content) + 8px (padding) = 38px total
    pub const HEIGHT: f32 = 30.0;

    /// Input padding: 4px
    pub const PADDING: f32 = Spacing::BASE_04;

    /// Input text size: 14px (matches TEXT_UI)
    pub const TEXT_SIZE: f32 = Typography::TEXT_UI;

    /// Input line height
    pub const LINE_HEIGHT: f32 = 30.0;

    /// Cursor width: 2px
    pub const CURSOR_WIDTH: f32 = 2.0;
}

/// Border radius constants
pub struct BorderRadius;

impl BorderRadius {
    /// Extra small: 2px
    pub const XS: f32 = 2.0;

    /// Small: 4px
    pub const SM: f32 = 4.0;

    /// Medium: 4px (confirmed from Zed)
    pub const MD: f32 = 4.0;

    /// Large: 8px
    pub const LG: f32 = 8.0;

    /// Extra large: 12px
    pub const XL: f32 = 12.0;

    /// Window decoration rounding: 10px
    pub const WINDOW: f32 = 10.0;
}
