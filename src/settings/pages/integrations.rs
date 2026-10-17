use amane::Service;

use super::super::page::Page;
use super::super::{Confirm, Settings, Shown, switch};

// the key, title, what the row says, and what turning it on does
const INTEGRATIONS: [(&str, &str, &str, &str); 13] = [
    (
        "integration_gtk",
        "GTK",
        "GTK 3/4 and desktop preferences",
        "This writes generated GTK themes, changes gtk-theme, icon-theme and color-scheme through dconf, replaces ~/.config/gtk-4.0/gtk.css with a symlink, and restarts xdg-desktop-portal-gnome.",
    ),
    (
        "integration_qt",
        "Qt / KDE",
        "Kate, Dolphin and other KDE apps",
        "This writes ~/.local/share/color-schemes/Amane.colors, replaces every color group in ~/.config/kdeglobals and sets ColorScheme=Amane there, then asks running KDE apps to reload.",
    ),
    (
        "integration_terminal",
        "Terminals",
        "Kitty and foot",
        "This overwrites the kitty and foot palette files, reloads kitty windows, signals all running foot processes, and writes color escape sequences to their terminals.",
    ),
    (
        "integration_alacritty",
        "Alacritty",
        "Imported colors, reloaded live",
        "This overwrites terminal-colors-alacritty.toml and adds it to the import list in ~/.config/alacritty/alacritty.toml. Colors set in alacritty.toml itself still win over imports.",
    ),
    (
        "integration_fish",
        "fish",
        "Syntax highlighting colors",
        "This sets fish_color_* and fish_pager_color_* as universal variables, replacing your current fish theme in every session.",
    ),
    (
        "integration_tmux",
        "tmux",
        "Status line and pane colors",
        "This overwrites tmux-colors.conf and asks running tmux servers to source that file.",
    ),
    (
        "integration_vesktop",
        "Vesktop",
        "Vencord Quick CSS",
        "This edits ~/.config/vesktop/settings/quickCss.css. Content outside the managed theme block is kept, but the file itself is rewritten.",
    ),
    (
        "integration_legcord",
        "Legcord",
        "Quick CSS",
        "This edits ~/.config/legcord/quickCss.css. Content outside the managed theme block is kept, but the file itself is rewritten.",
    ),
    (
        "integration_telegram",
        "Telegram",
        "freshGram, AyuGram and Telegram Desktop",
        "This writes ~/.local/state/amane/amane.tdesktop-theme with the wallpaper as chat background. Apply it once in Telegram (Settings → Chat Settings → Choose from file), after that it updates on its own.",
    ),
    (
        "integration_firefox",
        "Firefox",
        "Through the Pywalfox extension",
        "This overwrites ~/.cache/wal/colors.json, reinstalls the Pywalfox host in ~/.mozilla/native-messaging-hosts when it points at a missing program, and runs pywalfox update.",
    ),
    (
        "integration_spotify",
        "Spotify",
        "Spicetify color stylesheet",
        "This creates and overwrites the Spotify stylesheet under your cache directory.",
    ),
    (
        "integration_btop",
        "btop",
        "Generated terminal monitor theme",
        "This creates and overwrites ~/.config/btop/themes/amane.theme.",
    ),
    (
        "integration_cava",
        "Cava",
        "Generated visualizer theme",
        "This creates and overwrites ~/.config/cava/themes/amane.",
    ),
];

// turning one on asks first, turning it off doesn't
pub fn build(page: &mut Page) {
    for (key, title, detail, warning) in INTEGRATIONS {
        switch::add_with(page, key, title, detail, move || {
            if Settings::read().staged_flag(key) {
                Settings::stage(key, false);

                return;
            }

            Shown::write().confirm = Some(Confirm::Integration { key, title, warning });
        });
    }

    page.end_group();
}
