use super::super::button::{self, Style};
use super::super::page::Page;
use super::super::slider::{self, Range};
use amane::{Row, Service, TextInput};

use super::super::{Settings, field, row};
use crate::calendar::Calendar;

const KEY: &str = "calendar_urls";

/*
 * amane's text inputs can't paste, and a calendar link is far too long
 * to type, so the clipboard is read with wl-paste; a pasted link goes
 * after the ones already there
 */
fn paste() {
    let pasted = amane::output("timeout 2 wl-paste --no-newline --type text/plain 2>/dev/null");

    let pasted = pasted.trim();

    if pasted.is_empty() {
        return;
    }

    let current = String::from(Settings::read().staged(KEY).trim());

    let text = if current.is_empty() || current.contains(pasted) {
        String::from(pasted)
    } else {
        format!("{current} {pasted}")
    };

    set(&text);
}

fn clear() {
    set("");
}

fn set(text: &str) {
    Settings::stage(KEY, text);

    TextInput::set_text(KEY, text);
}

pub fn build(page: &mut Page) {
    field::add(
        page,
        KEY,
        "Calendar links",
        "Google Calendar → Settings → your calendar → Secret address in iCal format; several go apart with spaces",
    );

    let paste_label = "Paste";
    let clear_label = "Clear";

    let buttons = Row::new(amane::children![
        button::view(page.theme, paste_label, Style::Primary, true, paste),
        button::view(page.theme, clear_label, Style::Plain, true, clear),
    ])
    .gap(8.0);

    let buttons_width = button::width(paste_label) + button::width(clear_label) + 8.0;

    let paste_row = row::view(
        page.theme,
        page.width,
        row::HEIGHT,
        "Paste from clipboard",
        "Copy the link, press Paste, then Apply",
        buttons,
        buttons_width,
    );

    page.row(row::HEIGHT, paste_row);

    slider::add(
        page,
        "calendar_minutes",
        "Refresh interval",
        "How often the calendars are read again",
        Range {
            min: 5.0,
            max: 60.0,
            step: 5.0,
            label: slider::minutes,
        },
    );

    let label = "Refresh";

    let control = button::view(page.theme, label, Style::Plain, true, Calendar::refresh);

    let refresh = row::view(
        page.theme,
        page.width,
        row::HEIGHT,
        "Refresh now",
        "Read the calendars again without waiting",
        control,
        button::width(label),
    );

    page.row(row::HEIGHT, refresh);

    page.end_group();
}
