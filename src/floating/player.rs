use std::time::Duration;

use amane::{
    Center, Column, Full, Image, Media, Pointer, Rectangle, Row, Service, SpaceBetween,
    Start, Text, Weight, Widget, children,
};

use super::card;
use crate::fonts;
use crate::motion::{self, FAST_SPATIAL};
use crate::overlay::control_center::art;
use crate::theme::{self, Theme};

pub const WIDTH: f32 = 340.0;
pub const HEIGHT: f32 = 132.0;

const ART: f32 = 96.0;
const GAP: f32 = 14.0;

const PROGRESS_HEIGHT: f32 = 4.0;
const BUTTON: f32 = 30.0;
const PLAY_BUTTON: f32 = 34.0;

const EMPTY_ICON: &str = "󰝚";
const PREVIOUS_ICON: &str = "󰒮";
const PLAY_ICON: &str = "󰐊";
const PAUSE_ICON: &str = "󰏤";
const NEXT_ICON: &str = "󰒭";

// which button the pointer is on, the desktop cards have no other state to keep it in
#[derive(Default)]
pub struct Hovered {
    name: Option<&'static str>,
}

impl Service for Hovered {
    fn new() -> Self {
        Self::default()
    }

    // it only changes through input
    fn listen() {}
}

fn hover(name: &'static str, inside: bool) {
    let mut hovered = Hovered::write();

    if inside {
        hovered.name = Some(name);
    } else if hovered.name == Some(name) {
        hovered.name = None;
    }
}

// the cover beside what plays, a progress line and the playback buttons
pub fn view(theme: &Theme) -> Rectangle {
    let media = Media::read();

    let player = media.active();

    let (title, artist) = match player {
        Some(player) if !player.title().is_empty() => (player.title(), player.artist()),
        _ => ("Nothing playing", "Start some music"),
    };

    let playing = media.playing();

    let position = media.position();
    let length = media.length();

    let art_url = String::from(media.art_url());

    let title = String::from(title);
    let artist = String::from(artist);

    drop(media);

    let inner_width = WIDTH - 36.0;

    let details_width = inner_width - ART - GAP;

    let title = Text::new(title)
        .size(15.0)
        .font(fonts::BODY)
        .weight(Weight::Bold)
        .color(theme.text)
        .elide();

    let artist = Text::new(artist)
        .size(12.0)
        .font(fonts::BODY)
        .color(theme.secondary_text)
        .elide();

    let lines = Column::new(children![
        Rectangle::new().width(details_width).height(20.0).align_child(Start, Center).child(title),
        Rectangle::new().width(details_width).height(16.0).align_child(Start, Center).child(artist),
    ])
    .gap(2.0);

    let details = Column::new(children![
        lines,
        progress(theme, details_width, position, length),
        controls(theme, playing),
    ])
    .width(details_width)
    .height(ART)
    .justify(SpaceBetween);

    card::background(theme, WIDTH, HEIGHT)
        .align_child(Start, Center)
        .child(Row::new(children![cover(theme, &art_url), details]).gap(GAP).align(Center))
}

// a music note stands in until the cover is on disk and decoded
fn cover(theme: &Theme, url: &str) -> Rectangle {
    let slot = Rectangle::new()
        .width(ART)
        .height(ART)
        .radius(14.0)
        .fill(theme.border)
        .clip();

    if let Some(path) = art::path(url) {
        if Image::loaded(&path) {
            let pixels = ART as u32 * 2;

            return slot.fill(Image::cover(path).thumbnail(pixels, pixels));
        }
    }

    slot.align_child(Center, Center)
        .child(Text::new(EMPTY_ICON).size(40.0).font(fonts::NERD).tight().color(theme.muted_text))
}

// the played part over a track, with the times at its ends
fn progress(theme: &Theme, width: f32, position: Duration, length: Duration) -> Column {
    let played = if length.is_zero() {
        0.0
    } else {
        (position.as_secs_f32() / length.as_secs_f32()).clamp(0.0, 1.0)
    };

    let filled = Rectangle::new()
        .width(width * played)
        .height(PROGRESS_HEIGHT)
        .radius(Full)
        .fill(theme.accent);

    let track = Rectangle::new()
        .width(width)
        .height(PROGRESS_HEIGHT)
        .radius(Full)
        .fill(theme.border)
        .child(filled);

    let time = |duration: Duration| {
        let seconds = duration.as_secs();

        Text::new(format!("{}:{:02}", seconds / 60, seconds % 60))
            .size(10.0)
            .font(fonts::BODY)
            .color(theme.muted_text)
    };

    let times = Row::new(children![time(position), time(length)])
        .width(width)
        .justify(SpaceBetween);

    Column::new(children![track, times]).gap(3.0)
}

fn controls(theme: &Theme, playing: bool) -> Row {
    let play_icon = if playing { PAUSE_ICON } else { PLAY_ICON };

    let buttons: Vec<Box<dyn Widget>> = vec![
        Box::new(button(theme, "previous", PREVIOUS_ICON, Media::previous)),
        Box::new(play_button(theme, playing, play_icon)),
        Box::new(button(theme, "next", NEXT_ICON, Media::next)),
    ];

    Row::new(buttons).gap(6.0).align(Center)
}

// round, with a background only under the pointer
fn button(theme: &Theme, name: &'static str, icon: &str, action: fn()) -> Rectangle {
    let target = if Hovered::read().name == Some(name) { 1.0 } else { 0.0 };

    let amount = motion::fade(&format!("floating:player:{name}"), target);

    let fill = theme::with_opacity(theme.border, amount * 0.8);

    Rectangle::new()
        .width(BUTTON)
        .height(BUTTON)
        .radius(Full)
        .fill(fill)
        .cursor(Pointer)
        .on_hover(move |inside| hover(name, inside))
        .on_click(move |_| action())
        .align_child(Center, Center)
        .child(Text::new(icon).size(16.0).font(fonts::NERD).tight().color(theme.text))
}

// round while paused, a little squarer while playing
fn play_button(theme: &Theme, playing: bool, icon: &str) -> Rectangle {
    let target = if playing { 10.0 } else { PLAY_BUTTON / 2.0 };

    let radius = motion::follow("floating:player:play-radius", target, FAST_SPATIAL);

    let fill = if Hovered::read().name == Some("play") {
        theme::mix(theme.accent, theme.on_accent, 0.08)
    } else {
        theme.accent
    };

    Rectangle::new()
        .width(PLAY_BUTTON)
        .height(PLAY_BUTTON)
        .radius(radius)
        .fill(fill)
        .cursor(Pointer)
        .on_hover(|inside| hover("play", inside))
        .on_click(|_| Media::play_pause())
        .align_child(Center, Center)
        .child(Text::new(icon).size(17.0).font(fonts::NERD).tight().color(theme.on_accent))
}
