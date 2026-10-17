use std::time::{Duration, UNIX_EPOCH};

use amane::{Center, Column, Pointer, Rectangle, Row, Service, SpaceBetween, Start, Text, Weight, Widget, children};

use super::card;
use crate::calendar::{self, Calendar, Event, Status};
use crate::clock::Clock;
use crate::fonts;
use crate::theme::Theme;

pub const WIDTH: f32 = 280.0;
pub const HEIGHT: f32 = 204.0;

const ROWS: usize = 4;
const ROW_HEIGHT: f32 = 32.0;
const ROW_GAP: f32 = 4.0;

const MARKER_WIDTH: f32 = 3.0;

const ICON: &str = "\u{f00ed}";

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// opens the calendar in the browser
const WEB: &str = "https://calendar.google.com";

// the next few events from the calendar links, each with when it happens
pub fn view(theme: &Theme) -> Rectangle {
    let calendar = Calendar::read();

    let inner_width = WIDTH - 36.0;

    let header = Row::new(children![
        Text::new("UPCOMING").size(13.0).font(fonts::BODY).weight(Weight::SemiBold).color(theme.secondary_text),
        Text::new(ICON).size(18.0).font(fonts::NERD).tight().color(theme.secondary_text),
    ])
    .width(inner_width)
    .justify(SpaceBetween)
    .align(Center);

    let message = match calendar.status {
        Status::Unset => Some("Add a calendar link in Settings"),
        Status::Loading => Some("Loading…"),
        Status::Failed => Some("Couldn't reach the calendar"),
        Status::Ready => None,
    };

    let mut rows: Vec<Box<dyn Widget>> = Vec::new();

    if let Some(message) = message {
        rows.push(Box::new(Text::new(message).size(13.0).font(fonts::BODY).color(theme.muted_text)));
    } else {
        let upcoming = calendar.upcoming(ROWS);

        if upcoming.is_empty() {
            rows.push(Box::new(Text::new("Nothing planned").size(13.0).font(fonts::BODY).color(theme.muted_text)));
        }

        for event in upcoming {
            rows.push(Box::new(row(theme, event, inner_width)));
        }
    }

    let list = Column::new(rows).gap(ROW_GAP);

    card::background(theme, WIDTH, HEIGHT)
        .align_child(Start, Start)
        .cursor(Pointer)
        .on_click(|_| amane::spawn(&format!("xdg-open '{WEB}'")))
        .child(Column::new(children![header, list]).gap(10.0))
}

// a short accent line, then the title over when it is
fn row(theme: &Theme, event: &Event, width: f32) -> Row {
    let now = calendar::now();

    let happening = event.start <= now;

    let marker_color = if happening { theme.success } else { theme.accent };

    let marker = Rectangle::new()
        .width(MARKER_WIDTH)
        .height(ROW_HEIGHT - 4.0)
        .radius(MARKER_WIDTH / 2.0)
        .fill(marker_color);

    let text_width = width - MARKER_WIDTH - 10.0;

    let title = Text::new(event.title.as_str())
        .size(13.0)
        .font(fonts::BODY)
        .weight(Weight::SemiBold)
        .color(theme.text)
        .elide();

    let when = Text::new(when(event, happening))
        .size(11.0)
        .font(fonts::BODY)
        .color(theme.secondary_text)
        .elide();

    let lines = Column::new(children![
        Rectangle::new().width(text_width).height(16.0).align_child(Start, Center).child(title),
        Rectangle::new().width(text_width).height(14.0).align_child(Start, Center).child(when),
    ]);

    Row::new(children![marker, lines]).height(ROW_HEIGHT).gap(10.0).align(Center)
}

// "Now · until 15:30", "Today · 15:00 – 16:00", "Tomorrow · All day" or "Tue 21 · 09:00"
fn when(event: &Event, happening: bool) -> String {
    let clock = Clock::read();

    let moment = |unix: i64| UNIX_EPOCH + Duration::from_secs(unix.max(0) as u64);

    let today = clock.today_number();

    let day = clock.day_of(event.start);

    let day_name = match day - today {
        ..=0 => String::from("Today"),
        1 => String::from("Tomorrow"),
        _ => {
            let (_, _, date) = crate::clock::civil_date(day);

            // 1970-01-01 was a thursday, 3 days after a monday
            format!("{} {date}", WEEKDAYS[(day + 3).rem_euclid(7) as usize])
        }
    };

    if event.all_day {
        return format!("{day_name} · All day");
    }

    let start = clock.hours_minutes(moment(event.start));
    let end = clock.hours_minutes(moment(event.end));

    if happening {
        return format!("Now · until {end}");
    }

    if event.end > event.start {
        return format!("{day_name} · {start} – {end}");
    }

    format!("{day_name} · {start}")
}
