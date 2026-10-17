use std::process::Command;

use amane::Color;

use crate::theme::{self, Theme};

/*
 * fish's syntax colors, set as universal variables, which every running
 * fish picks up at its next prompt
 */
pub fn export(theme: &Theme) {
    let bare = |color: Color| super::hex(color).trim_start_matches('#').to_string();

    let quote = theme::mix(theme.success, theme.text, 0.2);

    let colors = [
        ("fish_color_normal", bare(theme.text)),
        ("fish_color_command", bare(theme.accent)),
        ("fish_color_keyword", bare(theme.accent_hover)),
        ("fish_color_param", bare(theme.text)),
        ("fish_color_option", bare(theme.secondary_text)),
        ("fish_color_quote", bare(quote)),
        ("fish_color_redirection", bare(theme.secondary_text)),
        ("fish_color_end", bare(theme.secondary_text)),
        ("fish_color_operator", bare(theme.accent)),
        ("fish_color_escape", bare(theme.accent_hover)),
        ("fish_color_error", bare(theme.danger)),
        ("fish_color_comment", bare(theme.muted_text)),
        ("fish_color_autosuggestion", bare(theme.muted_text)),
        ("fish_color_cancel", bare(theme.danger)),
        ("fish_color_valid_path", String::from("--underline")),
        ("fish_color_selection", format!("--background={}", bare(theme.selected_surface))),
        ("fish_color_search_match", format!("--background={}", bare(theme.selected_surface))),
        ("fish_pager_color_prefix", format!("{} --bold", bare(theme.accent))),
        ("fish_pager_color_completion", bare(theme.text)),
        ("fish_pager_color_description", bare(theme.muted_text)),
        ("fish_pager_color_progress", bare(theme.secondary_text)),
        ("fish_pager_color_selected_background", format!("--background={}", bare(theme.selected_surface))),
    ];

    let mut script = String::new();

    // only hex digits and flags go in, so nothing here needs quoting
    for (name, value) in colors {
        script.push_str(&format!("set -U {name} {value}; "));
    }

    if let Err(error) = Command::new("fish").args(["--no-config", "-c", &script]).status() {
        eprintln!("amane: failed to run fish for its colors: {error}");
    }
}
