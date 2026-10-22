use std::fs;

use super::{config_home, fill, hex, write};
use crate::theme::Theme;

const TEMPLATE: &str = include_str!("vesktop.css");

// midnight with the shell's colors, a whole theme rather than a token adapter
const THEME_TEMPLATE: &str = include_str!("vesktop.theme.css");

// the block this owns inside quick css, everything around it is kept
const START: &str = "/* amane-theme:start */";
const END: &str = "/* amane-theme:end */";

/*
 * a theme file vesktop lists under its themes and reloads on its own when
 * it changes; the adapter block left in quick css by older versions is cut
 * out, so it never fights the theme
 */
pub fn export(theme: &Theme) {
    let folder = format!("{}/vesktop", config_home());

    write(&format!("{folder}/themes/amane.theme.css"), &fill(THEME_TEMPLATE, &names(theme)));

    remove_block(&format!("{folder}/settings/quickCss.css"));
}

// legcord keeps a quick css of its own, which takes the same discord variables
pub fn export_legcord(theme: &Theme) {
    write_quick_css(&format!("{}/legcord/quickCss.css", config_home()), theme);
}

fn write_quick_css(path: &str, theme: &Theme) {
    let block = format!("{START}\n{}\n{END}\n", fill(TEMPLATE, &names(theme)).trim());

    let existing = fs::read_to_string(path).unwrap_or_default();

    let kept = without_block(&existing);

    let text = if kept.is_empty() {
        block
    } else {
        format!("{kept}\n\n{block}")
    };

    let _ = fs::write(path, text);
}

// quick css without this shell's block, left alone when it has none
fn remove_block(path: &str) {
    let Ok(existing) = fs::read_to_string(path) else {
        return;
    };

    if !existing.contains(START) {
        return;
    }

    let _ = fs::write(path, without_block(&existing));
}

// every @NAME@ the templates know
fn names(theme: &Theme) -> [(&'static str, String); 15] {
    let accent = theme.accent;

    let accent_rgb = format!("{}, {}, {}", accent.red(), accent.green(), accent.blue());

    let scheme = if theme.light { "light" } else { "dark" };

    [
        ("@COLOR_SCHEME@", String::from(scheme)),
        ("@BACKGROUND@", hex(theme.background)),
        ("@SURFACE@", hex(theme.surface)),
        ("@HOVER@", hex(theme.hover_surface)),
        ("@SELECTED@", hex(theme.selected_surface)),
        ("@BORDER@", hex(theme.border)),
        ("@TEXT@", hex(theme.text)),
        ("@SECONDARY@", hex(theme.secondary_text)),
        ("@MUTED@", hex(theme.muted_text)),
        ("@ACCENT_HOVER@", hex(theme.accent_hover)),
        ("@ACCENT_RGB@", accent_rgb),
        ("@ACCENT@", hex(accent)),
        ("@ON_ACCENT@", hex(theme.on_accent)),
        ("@SUCCESS@", hex(theme.success)),
        ("@DANGER@", hex(theme.danger)),
    ]
}

// the file with the old block cut out
fn without_block(text: &str) -> String {
    let (Some(start), Some(end)) = (text.find(START), text.find(END)) else {
        return String::from(text.trim());
    };

    if end < start {
        return String::from(text.trim());
    }

    let before = &text[..start];
    let after = &text[end + END.len()..];

    format!("{before}{after}").trim().to_string()
}
