use std::cell::RefCell;

use amane::{
    Battery, Center, Color, End, Full, Image, Memory, Padding, Parent, Pointer, Rectangle, Row, Service,
    SpaceBetween, Stack, Text, Widget, children,
};

use super::{pill, px, ring};
use crate::coffee;
use crate::dnd;
use crate::fonts;
use crate::keyboard::Keyboard;
use crate::tray::{self, Item, Tray};
use crate::motion::{self, Glide};
use crate::overlay::Overlay;
use crate::recorder;
use crate::settings::Settings;
use crate::theme::Theme;

// the memory icon, and the notification, wifi and bluetooth tray icons
const MEMORY_ICON: &str = "󰍛";
const TRAY_ICONS: [&str; 3] = ["󰂚", "󰖩", "󰂯"];

const RING_SIZE: f32 = 24.0;
const RING_THICKNESS: f32 = 3.0;

// 50px of icons with 10px of padding on each side
const TRAY_WIDTH: f32 = 70.0;
const TRAY_PADDING: f32 = 10.0;

// a toggle's circle inside its pill, and the room between two
const TOGGLE_SIZE: f32 = 22.0;
const TOGGLE_GAP: f32 = 4.0;

const COFFEE_OFF_ICON: &str = "\u{f06ca}";

// a tray icon, and the room between two
const TRAY_ICON: f32 = 18.0;
const TRAY_ICON_GAP: f32 = 8.0;

// with the row's 10px gap this keeps 15px from the screen edge
const EDGE: f32 = 5.0;

const RECORDING_DOT: &str = "\u{f0765}";

// milliseconds the tray takes to grow and shrink around the recording time
const RECORDING_DURATION: u64 = 420;

thread_local! {
    // kept outside services, a service write from view() would draw frames forever
    static RECORDING: RefCell<Option<(Glide, String)>> = const { RefCell::new(None) };
}

pub fn view(theme: &Theme, width: f32, monitor: &str) -> Row {
    let settings = Settings::read();

    let mut items: Vec<Box<dyn Widget>> = Vec::new();

    if settings.flag("bar_system_tray") {
        if let Some(tray) = system_tray(theme, monitor) {
            items.push(Box::new(tray));
        }
    }

    if settings.flag("bar_keyboard") {
        if let Some(layout) = layout(theme) {
            items.push(Box::new(layout));
        }
    }

    let coffee = settings.flag("bar_coffee");
    let quiet = settings.flag("bar_dnd");

    if coffee || quiet {
        items.push(Box::new(toggles(theme, coffee, quiet)));
    }

    if settings.flag("bar_battery") {
        if let Some(battery) = battery(theme) {
            items.push(Box::new(battery));
        }
    }

    if settings.flag("bar_memory") {
        items.push(Box::new(memory(theme)));
    }

    if settings.flag("bar_tray") {
        items.push(Box::new(tray(theme)));
    }

    items.push(Box::new(Rectangle::new().width(px(EDGE)).height(px(1.0))));

    Row::new(items)
        .width(width)
        .height(Parent)
        .gap(px(10.0))
        .justify(End)
        .align(Center)
}

/*
 * the icons programs put in the tray, in a pill of their own: left click
 * activates, right click opens the menu, middle click the second action;
 * none while no program shows anything
 */
fn system_tray(theme: &Theme, monitor: &str) -> Option<Rectangle> {
    let tray = Tray::read();

    let items: Vec<Item> = tray.items.iter().filter(|item| !item.passive).cloned().collect();

    drop(tray);

    if items.is_empty() {
        return None;
    }

    let mut icons: Vec<Box<dyn Widget>> = Vec::new();

    for item in items {
        icons.push(Box::new(tray_icon(theme, item, monitor)));
    }

    let count = icons.len() as f32;

    let width = px(TRAY_ICON * count + TRAY_ICON_GAP * (count - 1.0) + 16.0);

    let pill = pill::view(width, theme.surface)
        .align_child(Center, Center)
        .child(Row::new(icons).gap(px(TRAY_ICON_GAP)).align(Center));

    Some(pill)
}

fn tray_icon(theme: &Theme, item: Item, monitor: &str) -> Rectangle {
    let size = px(TRAY_ICON);

    let slot = Rectangle::new().width(size).height(size).radius(px(4.0));

    // a program's first letter stands in until its icon is found and decoded
    let slot = match &item.icon {
        Some(path) if Image::loaded(path) => {
            let pixels = (size * 2.0) as u32;

            slot.fill(Image::contain(path.clone()).thumbnail(pixels, pixels))
        }

        _ => {
            let letter: String = item.title.chars().take(1).collect::<String>().to_uppercase();

            slot.fill(theme.selected_surface)
                .align_child(Center, Center)
                .child(Text::new(letter).size(px(11.0)).font(fonts::BODY).color(theme.text))
        }
    };

    let monitor = String::from(monitor);

    let scrolled = item.clone();

    slot.cursor(Pointer)
        .on_click(move |button| tray::click(&item, button, &monitor))
        .on_scroll(move |scroll| tray::scroll(&scrolled, scroll))
}

// "EN" or "RU", switching to the next layout on click; none with a single layout
fn layout(theme: &Theme) -> Option<Rectangle> {
    let name = Keyboard::read().short_name()?;

    let size = px(13.0);

    let width = pill::text_width(&name, size) + px(20.0);

    let pill = pill::view(width, theme.surface)
        .cursor(Pointer)
        .on_click(|_| Keyboard::next())
        .align_child(Center, Center)
        .child(pill::label(&name, size, theme.text));

    Some(pill)
}

// coffee and do not disturb as round buttons in one pill, filled with the accent while on
fn toggles(theme: &Theme, coffee: bool, quiet: bool) -> Rectangle {
    let mut buttons: Vec<Box<dyn Widget>> = Vec::new();

    if coffee {
        let on = coffee::on();

        let icon = if on { coffee::ICON } else { COFFEE_OFF_ICON };

        buttons.push(Box::new(toggle(theme, icon, on, coffee::toggle)));
    }

    if quiet {
        let on = dnd::on();

        let icon = if on { dnd::ON_ICON } else { dnd::OFF_ICON };

        buttons.push(Box::new(toggle(theme, icon, on, dnd::toggle)));
    }

    let count = buttons.len() as f32;

    let width = px(TOGGLE_SIZE * count + TOGGLE_GAP * (count - 1.0) + 8.0);

    pill::view(width, theme.surface)
        .align_child(Center, Center)
        .child(Row::new(buttons).gap(px(TOGGLE_GAP)).align(Center))
}

fn toggle(theme: &Theme, icon: &str, on: bool, action: fn()) -> Rectangle {
    let (fill, color) = if on {
        (theme.accent, theme.on_accent)
    } else {
        (Color::TRANSPARENT, theme.text)
    };

    Rectangle::new()
        .width(px(TOGGLE_SIZE))
        .height(px(TOGGLE_SIZE))
        .radius(Full)
        .fill(fill)
        .cursor(Pointer)
        .on_click(move |_| action())
        .align_child(Center, Center)
        .child(Text::new(icon).size(px(14.0)).font(fonts::NERD).tight().color(color))
}

// a ring with a small icon inside, then the value
fn indicator(value: f32, color: Color, icon: &str, text: &str, theme: &Theme) -> Row {
    let ring = ring::view(px(RING_SIZE), px(RING_THICKNESS), value, color, theme.border);

    let icon = Rectangle::new()
        .width(px(RING_SIZE))
        .height(px(RING_SIZE))
        .align_child(Center, Center)
        .child(Text::new(icon).size(px(10.0)).font(fonts::MATERIAL).color(theme.text));

    let ring_with_icon = Stack::new(children![ring, icon]);

    Row::new(children![ring_with_icon, pill::label(text, px(14.0), theme.text)])
        .gap(px(6.0))
        .align(Center)
}

// none on a desktop without a battery
fn battery(theme: &Theme) -> Option<Row> {
    let battery = Battery::read();

    if !battery.present() {
        return None;
    }

    let percent = battery.percent();

    let color = if battery.charging() {
        theme.success
    } else if percent > 20 {
        theme.accent
    } else {
        theme.danger
    };

    let icon = battery_icon(percent, battery.charging());

    let text = format!("{percent}%");

    let value = f32::from(percent) / 100.0;

    Some(indicator(value, color, icon, &text, theme))
}

// one glyph per 10%
fn battery_icon(percent: u8, charging: bool) -> &'static str {
    if charging {
        return "󰚥";
    }

    match percent {
        91.. => "󰁹",
        81..=90 => "󰂂",
        71..=80 => "󰂁",
        61..=70 => "󰂀",
        51..=60 => "󰁿",
        41..=50 => "󰁾",
        31..=40 => "󰁽",
        21..=30 => "󰁼",
        11..=20 => "󰁻",
        _ => "󰁺",
    }
}

fn memory(theme: &Theme) -> Row {
    let memory = Memory::read();

    let usage = f32::from(memory.percent()) / 100.0;

    let color = if usage < 0.5 {
        theme.success
    } else if usage < 0.8 {
        theme.accent
    } else {
        theme.danger
    };

    // the used amount, like "5.2G"
    let gibibytes = memory.used_kib() as f32 / 1024.0 / 1024.0;

    let text = format!("{gibibytes:.1}G");

    indicator(usage, color, MEMORY_ICON, &text, theme)
}

// opens the utility center, and grows to the left with the time while a recording runs
fn tray(theme: &Theme) -> Rectangle {
    let mut icons: Vec<Box<dyn Widget>> = Vec::new();

    for icon in TRAY_ICONS {
        let text = Text::new(icon).size(px(15.0)).font(fonts::NERD).color(theme.on_accent);

        icons.push(Box::new(text));
    }

    // spread across the inside, so the spacing matches however wide amane measures each glyph
    let row = Row::new(icons)
        .width(px(TRAY_WIDTH - TRAY_PADDING * 2.0))
        .justify(SpaceBetween)
        .align(Center);

    let (extra, recording) = recording(theme);

    let content = Row::new(children![recording, row]).align(Center);

    pill::view(px(TRAY_WIDTH) + extra, theme.accent)
        .clip()
        .cursor(Pointer)
        .on_click(|_| Overlay::toggle_utility())
        .align_child(End, Center)
        .padding(Padding {
            top: 0.0,
            right: px(TRAY_PADDING),
            bottom: 0.0,
            left: 0.0,
        })
        .child(content)
}

/*
 * the dot and running time in front of the tray icons, and how much wider it makes the pill;
 * the width glides, and the last time stays while it closes
 */
fn recording(theme: &Theme) -> (f32, Rectangle) {
    let status = recorder::status();

    RECORDING.with_borrow_mut(|shown| {
        let (width, text) =
            shown.get_or_insert_with(|| (motion::spatial(0.0, RECORDING_DURATION), String::new()));

        if let Some(time) = status.as_ref() {
            text.clone_from(time);
        }

        // 10px of padding, the dot, a 6px gap, the time, then 8px before the icons
        let full = px(10.0 + 8.0 + 6.0 + 8.0) + pill::text_width(text, px(13.0));

        width.to(if status.is_some() { full } else { 0.0 });

        let extra = width.value();

        let dot = Text::new(RECORDING_DOT).size(px(8.0)).font(fonts::NERD).color(theme.on_accent);

        let segment = Rectangle::new()
            .width(extra)
            .height(Parent)
            .opacity(extra / full)
            .align_child(Center, Center)
            .child(
                Row::new(children![dot, pill::label(text, px(13.0), theme.on_accent)])
                    .gap(px(6.0))
                    .align(Center),
            );

        (extra, segment)
    })
}
