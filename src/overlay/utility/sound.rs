use amane::{
    Audio, AudioDevice, Center, Column, Padding, Pointer, Rectangle, Row, ScrollArea, Service,
    Start, Text, Weight, Widget, children,
};

use super::header::{self, Header};
use super::{hover, hovered};
use crate::fonts;
use crate::overlay::Overlay;
use crate::theme::Theme;

const LIST: &str = "sound-devices";

const GAP: f32 = 10.0;
const CARD_GAP: f32 = 7.0;

const CARD_HEIGHT: f32 = 50.0;
const CARD_PADDING: f32 = 9.0;
const ICON_SIZE: f32 = 32.0;

const OUTPUT_ICON: &str = "󰕾";
const OUTPUT_MUTED_ICON: &str = "󰝟";
const INPUT_ICON: &str = "󰍬";
const INPUT_MUTED_ICON: &str = "󰍭";
const SPEAKER_ICON: &str = "󰓃";
const CHECK: &str = "󰄬";

// icons for the kinds of device pulse names, anything else gets the side's own icon
const DEVICE_ICONS: [(&str, &str); 4] = [
    ("audio-headphones", "󰋋"),
    ("audio-headset", "󰋎"),
    ("audio-input-microphone", "󰍬"),
    ("video-display", "󰍹"),
];

// the speakers and the microphones, each under a header whose switch mutes it
pub fn view(overlay: &Overlay, theme: &Theme, width: f32, height: f32) -> Column {
    let audio = Audio::read();

    let output = Side {
        name: "sound:output",
        title: "Output",
        icon: if audio.muted() { OUTPUT_MUTED_ICON } else { OUTPUT_ICON },
        fallback_icon: SPEAKER_ICON,
        volume: audio.volume(),
        muted: audio.muted(),
        devices: audio.outputs(),
        toggle_mute: Audio::toggle_mute,
        pick: Audio::set_output,
    };

    let input = Side {
        name: "sound:input",
        title: "Input",
        icon: if audio.microphone_muted() { INPUT_MUTED_ICON } else { INPUT_ICON },
        fallback_icon: INPUT_ICON,
        volume: audio.microphone_volume(),
        muted: audio.microphone_muted(),
        devices: audio.inputs(),
        toggle_mute: Audio::toggle_microphone_mute,
        pick: Audio::set_input,
    };

    let mut parts: Vec<Box<dyn Widget>> = Vec::new();

    let mut total = 0.0;

    for side in [output, input] {
        let (column, side_height) = side_view(overlay, theme, &side, width);

        parts.push(Box::new(column));

        total += side_height;
    }

    total += GAP * 2.0;

    let column = Column::new(parts).width(width).height(total).gap(GAP * 2.0);

    let area = Rectangle::new()
        .width(width)
        .height(height)
        .child(ScrollArea::new(LIST, column));

    Column::new(children![area])
}

// one direction of sound, its levels and the devices it can go through
struct Side<'a> {
    name: &'static str,
    title: &'static str,
    icon: &'static str,
    fallback_icon: &'static str,
    volume: u8,
    muted: bool,
    devices: &'a [AudioDevice],
    toggle_mute: fn(),
    pick: fn(&str),
}

// the header, then a card per device; gives back how tall it all is
fn side_view(overlay: &Overlay, theme: &Theme, side: &Side, width: f32) -> (Column, f32) {
    let status = if side.devices.is_empty() {
        String::from("No devices")
    } else if side.muted {
        String::from("Muted")
    } else {
        format!("{}%", side.volume)
    };

    let header = Header {
        name: side.name,
        icon: side.icon,
        title: side.title,
        status: &status,
        status_color: theme.muted_text,
        active: !side.muted,
    };

    let toggle_mute = side.toggle_mute;

    let header = header::view(overlay, theme, header, width, toggle_mute);

    let mut parts: Vec<Box<dyn Widget>> = vec![Box::new(header)];

    for device in side.devices {
        parts.push(Box::new(card(overlay, theme, side, device, width)));
    }

    let count = side.devices.len() as f32;

    let height = header::HEIGHT + count * (CARD_HEIGHT + CARD_GAP);

    (Column::new(parts).width(width).height(height).gap(CARD_GAP), height)
}

// one device, tinted with a check while it is the default, a click makes it the default
fn card(overlay: &Overlay, theme: &Theme, side: &Side, device: &AudioDevice, width: f32) -> Rectangle {
    let selected = device.default();

    let hover_name = format!("{}:{}", side.name, device.name());

    let fill = if selected {
        theme.selected_surface
    } else if hovered(overlay, &hover_name) {
        theme.hover_surface
    } else {
        theme.surface
    };

    let icon = Rectangle::new()
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .radius(8.0)
        .fill(theme.selected_surface)
        .align_child(Center, Center)
        .child(
            Text::new(device_icon(device.icon(), side.fallback_icon))
                .size(16.0)
                .font(fonts::NERD)
                .tight()
                .color(theme.accent),
        );

    let weight = if selected { Weight::SemiBold } else { Weight::Regular };

    let text_width = width - CARD_PADDING * 4.0 - ICON_SIZE - 16.0;

    let name = Text::new(device.description())
        .size(12.0)
        .font(fonts::BODY)
        .weight(weight)
        .color(theme.text)
        .elide();

    let name = Rectangle::new()
        .width(text_width)
        .height(ICON_SIZE)
        .align_child(Start, Center)
        .child(name);

    let check = if selected { CHECK } else { "" };

    let check = Text::new(check).size(14.0).font(fonts::NERD).tight().color(theme.accent);

    let card = Rectangle::new()
        .width(width)
        .height(CARD_HEIGHT)
        .radius(12.0)
        .fill(fill)
        .padding(Padding {
            top: CARD_PADDING,
            right: CARD_PADDING * 2.0,
            bottom: CARD_PADDING,
            left: CARD_PADDING,
        })
        .child(Row::new(children![icon, name, check]).gap(CARD_PADDING).align(Center));

    // the default needs no click, picking it again would do nothing
    if selected {
        return card;
    }

    let pick = side.pick;
    let device = String::from(device.name());

    card.cursor(Pointer)
        .on_hover(move |inside| hover(hover_name.clone(), inside))
        .on_click(move |_| pick(&device))
}

// pulse names kinds like "audio-headphones" or "audio-headset-bluetooth"
fn device_icon(kind: &str, fallback: &'static str) -> &'static str {
    for (prefix, icon) in DEVICE_ICONS {
        if kind.starts_with(prefix) {
            return icon;
        }
    }

    fallback
}
