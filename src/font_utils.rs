//! Font Utilities Module
//!
//! Provides a list of common system fonts for selection.

/// Get a list of common system fonts
///
/// Returns a list of commonly available fonts across Windows, macOS, and Linux.
/// These fonts should be available via system font providers (DirectWrite, CoreText, fontconfig).
pub fn get_system_fonts() -> Vec<String> {
    vec![
        "Arial".to_string(),
        "Arial Black".to_string(),
        "Calibri".to_string(),
        "Cambria".to_string(),
        "Comic Sans MS".to_string(),
        "Consolas".to_string(),
        "Courier New".to_string(),
        "DejaVu Sans".to_string(),
        "DejaVu Sans Mono".to_string(),
        "DejaVu Serif".to_string(),
        "Georgia".to_string(),
        "Helvetica".to_string(),
        "Impact".to_string(),
        "Liberation Sans".to_string(),
        "Liberation Serif".to_string(),
        "Lucida Console".to_string(),
        "Noto Sans".to_string(),
        "Segoe UI".to_string(),
        "Tahoma".to_string(),
        "Times New Roman".to_string(),
        "Trebuchet MS".to_string(),
        "Verdana".to_string(),
    ]
}
