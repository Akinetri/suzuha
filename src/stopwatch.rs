mod history;
mod ring;

use std::cell::RefCell;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use amane::{
    Center, Color, Column, End, Full, Key, Parent, Pointer, Rectangle, Row, Service,
    SpaceBetween, Stack, Start, Text, Weight, Widget, Window, children,
};

use crate::clock::Clock;
use crate::fonts;
use crate::motion::{self, Glide};
use crate::theme::{self, Theme};

use history::Session;

// what open_window and close_window know this window by
const NAME: &str = "stopwatch";

const WIDTH: f32 = 480.0;
const HEIGHT: f32 = 800.0;

const MARGIN: f32 = 22.0;

// the space between the ring, the controls and the statistics
const GAP: f32 = 22.0;

const PLAY_HEIGHT: f32 = 64.0;

const TILE_HEIGHT: f32 = 74.0;
const TILE_GAP: f32 = 10.0;

const WEEK_HEIGHT: f32 = 170.0;
const WEEK_PADDING: f32 = 16.0;
const WEEK_TITLE: f32 = 22.0;
const DAY_LABEL: f32 = 16.0;
const BAR_WIDTH: f32 = 26.0;

// a week of less than this still draws its bars against a full hour
const SMALLEST_SCALE: u64 = 3600;

// the ring fills once an hour
const RING_PERIOD: f32 = 3600.0;

pub const TIMER_ICON: &str = "󰔛";
const CLOSE_ICON: &str = "󰅖";
pub const PLAY_ICON: &str = "\u{f040a}";
pub const PAUSE_ICON: &str = "\u{f03e4}";
const STOP_ICON: &str = "\u{f04db}";

const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

thread_local! {
    // kept outside services, a service write from view() would draw frames forever
    static MORPHS: RefCell<Vec<Glide>> = const { RefCell::new(Vec::new()) };
}

/*
 * one long focus session that counts up; stopping it saves it to the
 * history, which the window sums up by day, week and all time
 */
pub struct Stopwatch {
    // counted before the current run, across pauses
    banked: Duration,

    // when the current run began, only while running
    since: Option<Instant>,

    // when the session began in seconds since 1970, none before the first start
    started: Option<i64>,

    history: Vec<Session>,

    hovered: Option<String>,
}

impl Service for Stopwatch {
    // a session that was under way when the shell stopped picks up where it was
    fn new() -> Self {
        let mut stopwatch = Self {
            banked: Duration::ZERO,
            since: None,
            started: None,
            history: history::load(),
            hovered: None,
        };

        if let Some((started, banked, running_since)) = history::load_current() {
            let mut banked = banked;

            if let Some(since) = running_since {
                banked += (now() - since).max(0) as u64;

                stopwatch.since = Some(Instant::now());
            }

            stopwatch.banked = Duration::from_secs(banked);
            stopwatch.started = Some(started);
        }

        stopwatch
    }

    // keeps counting with the window closed
    fn update(&mut self) -> bool {
        self.since.is_some()
    }
}

impl Stopwatch {
    fn elapsed(&self) -> Duration {
        self.banked + self.since.map_or(Duration::ZERO, |since| since.elapsed())
    }

    fn running(&self) -> bool {
        self.since.is_some()
    }

    fn save(&self) {
        let Some(started) = self.started else {
            history::clear_current();

            return;
        };

        let running_since = self.since.map(|since| now() - since.elapsed().as_secs() as i64);

        history::save_current(started, self.banked.as_secs(), running_since);
    }

    // seconds counted on each local day, for every session and the one under way
    fn totals(&self) -> Totals {
        let clock = Clock::read();

        let today = clock.today_number();

        // 1970-01-01 was a thursday, 3 days after a monday
        let monday = today - (today + 3).rem_euclid(7);

        let mut totals = Totals::default();

        let current = self.started.map(|started| Session {
            started,
            seconds: self.elapsed().as_secs(),
        });

        for session in self.history.iter().copied().chain(current) {
            let day = clock.day_of(session.started);

            totals.all_time += session.seconds;
            totals.longest = totals.longest.max(session.seconds);

            if day == today {
                totals.today += session.seconds;
            }

            if (monday..monday + 7).contains(&day) {
                totals.week[(day - monday) as usize] += session.seconds;
            }
        }

        totals.sessions = self.history.len();
        totals.weekday = (today - monday) as usize;

        totals
    }
}

#[derive(Default)]
struct Totals {
    today: u64,

    // monday first
    week: [u64; 7],

    // which of the week's days is today
    weekday: usize,

    all_time: u64,
    longest: u64,

    // finished ones only
    sessions: usize,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

pub fn open() {
    amane::open_window(NAME, view);
}

fn close() {
    amane::close_window(NAME);
}

// starts a new session, or pauses and resumes the one under way
pub fn toggle() {
    let mut stopwatch = Stopwatch::write();

    match stopwatch.since.take() {
        Some(since) => stopwatch.banked += since.elapsed(),

        None => {
            stopwatch.since = Some(Instant::now());

            if stopwatch.started.is_none() {
                stopwatch.started = Some(now());
            }
        }
    }

    stopwatch.save();
}

// the session goes into the history, and the stopwatch back to zero
pub fn stop() {
    let mut stopwatch = Stopwatch::write();

    let seconds = stopwatch.elapsed().as_secs();

    if let Some(started) = stopwatch.started.take() {
        if seconds >= history::SHORTEST {
            let session = Session { started, seconds };

            history::add(session);

            stopwatch.history.push(session);
        }
    }

    stopwatch.banked = Duration::ZERO;
    stopwatch.since = None;

    stopwatch.save();
}

// "toggle", "stop" or "open"
pub fn ipc(arguments: &[String]) -> String {
    match arguments.first().map(String::as_str) {
        Some("stop") => stop(),
        Some("open") => open(),
        _ => toggle(),
    }

    String::from("ok")
}

fn hover(name: String, inside: bool) {
    let mut stopwatch = Stopwatch::write();

    if inside {
        stopwatch.hovered = Some(name);
    } else if stopwatch.hovered.as_ref() == Some(&name) {
        stopwatch.hovered = None;
    }
}

fn hovered(name: &str) -> bool {
    Stopwatch::read().hovered.as_deref() == Some(name)
}

fn key_pressed(key: Key) {
    match key {
        Key::Escape => close(),
        Key::Space => toggle(),
        _ => {}
    }
}

pub fn view() -> Window {
    let theme = theme::current();

    let (width, height) = match amane::window_size() {
        (0.0, _) | (_, 0.0) => (WIDTH, HEIGHT),
        size => size,
    };

    let inner_width = width - MARGIN * 2.0;
    let inner_height = height - MARGIN * 2.0;

    let stats_height = TILE_HEIGHT + TILE_GAP + WEEK_HEIGHT;

    // what the header, controls, statistics and gaps leave for the ring
    let room = inner_height - 40.0 - PLAY_HEIGHT - stats_height - GAP * 4.0;

    let ring_size = room.min(inner_width).clamp(160.0, 360.0);

    let totals = Stopwatch::read().totals();

    let body = Column::new(children![
        dial(&theme, ring_size),
        controls(&theme),
        statistics(&theme, &totals, inner_width),
    ])
    .gap(GAP)
    .align(Center);

    let body = Rectangle::new()
        .width(inner_width)
        .height(inner_height - 40.0)
        .align_child(Center, Center)
        .child(body);

    let content = Column::new(children![header(&theme, inner_width), body]);

    let frame = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .fill(theme.background)
        .padding(MARGIN)
        .child(content);

    Window::new()
        .title("Stopwatch")
        .size(WIDTH, HEIGHT)
        .on_key(key_pressed)
        .child(frame)
}

fn header(theme: &Theme, width: f32) -> Row {
    let icon = Rectangle::new()
        .width(40.0)
        .height(40.0)
        .radius(12.0)
        .fill(theme.selected_surface)
        .align_child(Center, Center)
        .child(Text::new(TIMER_ICON).size(21.0).font(fonts::NERD).tight().color(theme.accent));

    let title = Text::new("Stopwatch")
        .size(21.0)
        .font(fonts::BODY)
        .weight(Weight::Bold)
        .color(theme.text);

    let close_fill = if hovered("close") {
        theme.hover_surface
    } else {
        Color::TRANSPARENT
    };

    let close = Rectangle::new()
        .width(40.0)
        .height(40.0)
        .radius(Full)
        .fill(close_fill)
        .cursor(Pointer)
        .on_hover(|inside| hover(String::from("close"), inside))
        .on_click(|_| close())
        .align_child(Center, Center)
        .child(Text::new(CLOSE_ICON).size(18.0).font(fonts::NERD).tight().color(theme.secondary_text));

    Row::new(children![Row::new(children![icon, title]).gap(12.0).align(Center), close])
        .width(width)
        .height(40.0)
        .justify(SpaceBetween)
        .align(Center)
}

pub fn running() -> bool {
    Stopwatch::read().running()
}

// like "1:23:45" while a session is under way, for the bar
pub fn status() -> Option<String> {
    let stopwatch = Stopwatch::read();

    stopwatch.started?;

    Some(time(stopwatch.elapsed()))
}

// "23:45", with the hours in front once there are any
fn time(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();

    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;

    if hours == 0 {
        return format!("{minutes:02}:{:02}", seconds % 60);
    }

    format!("{hours}:{minutes:02}:{:02}", seconds % 60)
}

// the time so far inside a wavy ring that fills once an hour
fn dial(theme: &Theme, size: f32) -> Stack {
    let stopwatch = Stopwatch::read();

    let elapsed = stopwatch.elapsed();

    let running = stopwatch.running();

    let state = match (running, stopwatch.started) {
        (true, _) => "Focusing",
        (false, Some(_)) => "Paused",
        (false, None) => "Ready",
    };

    drop(stopwatch);

    let hour = elapsed.as_secs_f32() % RING_PERIOD / RING_PERIOD;

    let readout = Column::new(children![
        Text::new(time(elapsed)).size(size * 0.2).font(fonts::BODY).weight(Weight::Bold).tight().color(theme.text),
        Text::new(state).size(14.0).font(fonts::BODY).weight(Weight::SemiBold).color(theme.accent),
    ])
    .gap(8.0)
    .align(Center);

    let middle = Rectangle::new()
        .width(size)
        .height(size)
        .align_child(Center, Center)
        .child(readout);

    Stack::new(children![ring::view(size, hour, running, theme.accent, theme.selected_surface), middle])
        .width(size)
        .height(size)
}

// stop beside a big play button that squares off while running
fn controls(theme: &Theme) -> Row {
    let stopwatch = Stopwatch::read();

    let running = stopwatch.running();
    let started = stopwatch.started.is_some();

    drop(stopwatch);

    let (icon, radius) = if running {
        (PAUSE_ICON, 22.0)
    } else {
        (PLAY_ICON, PLAY_HEIGHT / 2.0)
    };

    let play_fill = if hovered("play") {
        theme::mix(theme.accent, theme.on_accent, 0.08)
    } else {
        theme.accent
    };

    let play = Rectangle::new()
        .width(112.0)
        .height(PLAY_HEIGHT)
        .radius(morph(0, radius))
        .fill(play_fill)
        .cursor(Pointer)
        .on_hover(|inside| hover(String::from("play"), inside))
        .on_click(|_| toggle())
        .align_child(Center, Center)
        .child(Text::new(icon).size(30.0).font(fonts::NERD).tight().color(theme.on_accent));

    let stop_fill = if hovered("stop") && started {
        theme::mix(theme.selected_surface, theme.text, 0.08)
    } else {
        theme.selected_surface
    };

    // nothing to stop before the first start
    let stop_color = if started { theme.text } else { theme.muted_text };

    let mut stop_button = Rectangle::new()
        .width(PLAY_HEIGHT)
        .height(PLAY_HEIGHT)
        .radius(Full)
        .fill(stop_fill)
        .on_hover(|inside| hover(String::from("stop"), inside))
        .align_child(Center, Center)
        .child(Text::new(STOP_ICON).size(24.0).font(fonts::NERD).tight().color(stop_color));

    if started {
        stop_button = stop_button.cursor(Pointer).on_click(|_| stop());
    }

    Row::new(children![stop_button, play]).gap(12.0).align(Center)
}

// today, this week and all time as tiles, over the week day by day
fn statistics(theme: &Theme, totals: &Totals, width: f32) -> Column {
    let week: u64 = totals.week.iter().sum();

    let tile_width = (width - TILE_GAP * 2.0) / 3.0;

    let sessions = match totals.sessions {
        1 => String::from("1 session"),
        count => format!("{count} sessions"),
    };

    let tiles = Row::new(children![
        tile(theme, tile_width, "TODAY", &history::duration(totals.today), None),
        tile(theme, tile_width, "THIS WEEK", &history::duration(week), None),
        tile(theme, tile_width, "ALL TIME", &history::duration(totals.all_time), Some(&sessions)),
    ])
    .gap(TILE_GAP);

    Column::new(children![tiles, week_chart(theme, totals, width)]).gap(TILE_GAP)
}

fn tile(theme: &Theme, width: f32, label: &str, value: &str, detail: Option<&str>) -> Rectangle {
    let label = Text::new(label)
        .size(11.0)
        .font(fonts::BODY)
        .weight(Weight::SemiBold)
        .color(theme.secondary_text);

    let value = Text::new(value)
        .size(20.0)
        .font(fonts::BODY)
        .weight(Weight::Bold)
        .color(theme.text);

    let mut lines: Vec<Box<dyn Widget>> = vec![Box::new(label), Box::new(value)];

    if let Some(detail) = detail {
        lines.push(Box::new(Text::new(detail).size(10.0).font(fonts::BODY).color(theme.muted_text)));
    }

    Rectangle::new()
        .width(width)
        .height(TILE_HEIGHT)
        .radius(16.0)
        .fill(theme.surface)
        .padding(12.0)
        .align_child(Start, Center)
        .child(Column::new(lines).gap(2.0))
}

// a bar per day from monday, today in the accent, scaled to the busiest day
fn week_chart(theme: &Theme, totals: &Totals, width: f32) -> Rectangle {
    let inner_width = width - WEEK_PADDING * 2.0;
    let inner_height = WEEK_HEIGHT - WEEK_PADDING * 2.0;

    let bars_height = inner_height - WEEK_TITLE - DAY_LABEL - 8.0;

    let scale = totals.week.iter().copied().max().unwrap_or(0).max(SMALLEST_SCALE) as f32;

    let title = Row::new(children![
        Text::new("This week").size(13.0).font(fonts::BODY).weight(Weight::SemiBold).color(theme.text),
        Text::new(format!("longest {}", history::duration(totals.longest)))
            .size(11.0)
            .font(fonts::BODY)
            .color(theme.muted_text),
    ])
    .width(inner_width)
    .height(WEEK_TITLE)
    .justify(SpaceBetween)
    .align(Center);

    let column_width = inner_width / 7.0;

    let mut days: Vec<Box<dyn Widget>> = Vec::new();

    for (index, seconds) in totals.week.iter().enumerate() {
        let today = index == totals.weekday;

        let share = *seconds as f32 / scale;

        // a day with anything at all shows at least a sliver
        let filled = if *seconds > 0 { (bars_height * share).max(4.0) } else { 0.0 };

        let fill = if today {
            theme.accent
        } else {
            theme::mix(theme.selected_surface, theme.accent, 0.45)
        };

        let bar = Rectangle::new()
            .width(BAR_WIDTH)
            .height(filled)
            .radius(6.0)
            .fill(fill);

        let track = Rectangle::new()
            .width(BAR_WIDTH)
            .height(bars_height)
            .radius(6.0)
            .fill(theme.selected_surface)
            .align_child(Center, End)
            .child(bar);

        let label_color = if today { theme.accent } else { theme.muted_text };

        let label = Text::new(WEEKDAYS[index])
            .size(10.0)
            .font(fonts::BODY)
            .weight(Weight::SemiBold)
            .color(label_color);

        let column = Column::new(children![
            track,
            Rectangle::new().width(column_width).height(DAY_LABEL).align_child(Center, Center).child(label),
        ])
        .width(column_width)
        .gap(8.0)
        .align(Center);

        days.push(Box::new(column));
    }

    Rectangle::new()
        .width(width)
        .height(WEEK_HEIGHT)
        .radius(16.0)
        .fill(theme.surface)
        .padding(WEEK_PADDING)
        .child(Column::new(children![title, Row::new(days)]).gap(0.0))
}

// a corner radius that glides to its target, one per shape
fn morph(index: usize, radius: f32) -> f32 {
    MORPHS.with_borrow_mut(|morphs| {
        while morphs.len() <= index {
            morphs.push(motion::spatial(radius, motion::DEFAULT_SPATIAL));
        }

        morphs[index].to(radius);

        morphs[index].value()
    })
}
