use std::fs;
use std::path::Path;

use amane::Service;

use super::{hex, home, write};
use crate::theme::Theme;
use crate::wallpaper::Wallpaper;

/*
 * firefox through the pywalfox extension, which reads pywal's colors
 * file; its palette is pywal's: the background first, the accents
 * after it and the text last, not the terminal's ansi order
 */
pub fn export(theme: &Theme) {
    let colors = [
        theme.background,
        theme.accent,
        theme.accent_hover,
        theme.success,
        theme.danger,
        theme.selected_surface,
        theme.secondary_text,
        theme.text,
        theme.surface,
        theme.accent,
        theme.accent_hover,
        theme.success,
        theme.danger,
        theme.border,
        theme.muted_text,
        theme.text,
    ];

    let mut entries = Vec::new();

    for (index, color) in colors.iter().enumerate() {
        entries.push(format!("    \"color{index}\": \"{}\"", hex(*color)));
    }

    // a path with a quote or backslash would break the json, and pywalfox only shows it anyway
    let wallpaper = Wallpaper::read().path.replace(['"', '\\'], "");

    let text = format!(
        "{{\n  \"wallpaper\": \"{wallpaper}\",\n  \"alpha\": \"100\",\n  \"special\": {{\n    \"background\": \"{}\",\n    \"foreground\": \"{}\",\n    \"cursor\": \"{}\"\n  }},\n  \"colors\": {{\n{}\n  }}\n}}\n",
        hex(theme.background),
        hex(theme.text),
        hex(theme.accent),
        entries.join(",\n"),
    );

    write(&format!("{}/.cache/wal/colors.json", home()), &text);

    // the extension is told after the host is fixed, or it would have nobody to hear it
    let install = if host_works() { "" } else { "pywalfox install >/dev/null 2>&1; " };

    amane::spawn(&format!("{install}pywalfox update >/dev/null 2>&1"));
}

/*
 * firefox finds pywalfox through a small manifest; another tool can leave
 * it pointing at a program that is gone, and then the extension never connects
 */
fn host_works() -> bool {
    let manifest = format!("{}/.mozilla/native-messaging-hosts/pywalfox.json", home());

    let Ok(text) = fs::read_to_string(&manifest) else {
        return false;
    };

    let program = text
        .split("\"path\"")
        .nth(1)
        .and_then(|rest| rest.split('"').nth(1))
        .unwrap_or_default();

    Path::new(program).exists()
}
