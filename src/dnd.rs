use amane::Service;

use crate::settings::Settings;

pub const ON_ICON: &str = "\u{f009b}";
pub const OFF_ICON: &str = "\u{f009a}";

/*
 * do not disturb: notifications still arrive and wait in the utility
 * panel, only the popups stay away; kept in the settings across restarts
 */
pub fn on() -> bool {
    Settings::read().flag("do_not_disturb")
}

pub fn toggle() {
    Settings::set("do_not_disturb", !on());
}

// "toggle", "on" or "off"
pub fn ipc(arguments: &[String]) -> String {
    match arguments.first().map(String::as_str) {
        Some("on") => Settings::set("do_not_disturb", true),
        Some("off") => Settings::set("do_not_disturb", false),
        _ => toggle(),
    }

    String::from(if on() { "on" } else { "off" })
}
