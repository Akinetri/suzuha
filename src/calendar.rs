mod ics;

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use amane::Service;

use crate::clock::{Clock, civil_date};
use crate::settings::Settings;

use ics::{Moment, Zone};

// how often the loop looks at the settings and the refresh button between polls
const CHECK: Duration = Duration::from_secs(1);

// days before today and after it that are read, enough for the month around today
const DAYS_BEFORE: i64 = 40;
const DAYS_AFTER: i64 = 70;

const DAY: i64 = 24 * 60 * 60;

// set by the refresh button, the next check asks at once
static REFRESH: AtomicBool = AtomicBool::new(false);

#[derive(Clone, PartialEq)]
pub struct Event {
    pub title: String,

    // seconds since 1970
    pub start: i64,
    pub end: i64,

    pub all_day: bool,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Status {
    // no link in the settings
    #[default]
    Unset,

    Loading,
    Ready,

    // not one of the links could be read
    Failed,
}

/*
 * the events from the calendar links in the settings, like the secret
 * ical address of a google calendar; polled off the draw thread
 */
#[derive(Default, PartialEq)]
pub struct Calendar {
    pub status: Status,

    // by start
    pub events: Vec<Event>,
}

impl Service for Calendar {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {
        loop {
            let links = links();

            if links.is_empty() {
                *Self::write() = Self::default();
            } else {
                if Self::read().status == Status::Unset {
                    Self::write().status = Status::Loading;
                }

                let fresh = fetch(&links);

                // offline keeps what was there, so a short drop doesn't empty the card
                if fresh.status == Status::Ready || Self::read().status != Status::Ready {
                    if *Self::read() != fresh {
                        *Self::write() = fresh;
                    }
                }
            }

            let started = Instant::now();

            loop {
                thread::sleep(CHECK);

                let minutes = Settings::read().number("calendar_minutes").max(1.0);

                let due = started.elapsed().as_secs_f32() >= minutes * 60.0;

                if due || self::links() != links || REFRESH.swap(false, Ordering::Relaxed) {
                    break;
                }
            }
        }
    }
}

impl Calendar {
    // from the settings' refresh button
    pub fn refresh() {
        REFRESH.store(true, Ordering::Relaxed);
    }

    // the events not over yet, soonest first
    pub fn upcoming(&self, count: usize) -> Vec<&Event> {
        let now = now();

        self.events.iter().filter(|event| event.end > now).take(count).collect()
    }

    // the local days, in days since 1970, that have anything on them
    pub fn busy_days(&self) -> BTreeSet<i64> {
        let clock = Clock::read();

        let mut days = BTreeSet::new();

        for event in &self.events {
            let first = clock.day_of(event.start);

            // an event ending at midnight doesn't reach into the next day
            let last = clock.day_of((event.end - 1).max(event.start));

            for day in first..=last.min(first + 60) {
                days.insert(day);
            }
        }

        days
    }
}

// the links go into a shell command inside single quotes, so only plain web links are let through
fn links() -> Vec<String> {
    let settings = Settings::read();

    settings
        .text("calendar_urls")
        .split([' ', ',', '\n'])
        .map(str::trim)
        .filter(|link| link.starts_with("https://") || link.starts_with("http://"))
        .filter(|link| !link.contains('\''))
        .map(String::from)
        .collect()
}

fn fetch(links: &[String]) -> Calendar {
    let today = Clock::read().today_number();

    let from = today - DAYS_BEFORE;
    let to = today + DAYS_AFTER;

    let mut occurrences = Vec::new();
    let mut read_any = false;

    for link in links {
        let text = amane::output(&format!("curl -sfL --max-time 30 '{link}'"));

        if !text.contains("BEGIN:VCALENDAR") {
            eprintln!("amane: failed to read the calendar at one of the links");

            continue;
        }

        read_any = true;

        occurrences.extend(ics::occurrences(&text, from, to));
    }

    if !read_any {
        return Calendar {
            status: Status::Failed,
            events: Vec::new(),
        };
    }

    let starts = to_unix(&occurrences.iter().map(|occurrence| occurrence.start.clone()).collect::<Vec<_>>());

    let mut events = Vec::new();

    for (occurrence, start) in occurrences.into_iter().zip(starts) {
        events.push(Event {
            all_day: occurrence.start.time.is_none(),
            title: occurrence.title,
            start,
            end: start + occurrence.length,
        });
    }

    events.sort_by(|first, second| first.start.cmp(&second.start).then(first.title.cmp(&second.title)));
    events.dedup();

    Calendar {
        status: Status::Ready,
        events,
    }
}

/*
 * every moment in seconds since 1970; a wall clock in a named zone is
 * handed to `date` in one batch, which knows each zone's daylight saving
 */
fn to_unix(moments: &[Moment]) -> Vec<i64> {
    let mut lines = String::new();

    for moment in moments {
        let (year, month, day) = civil_date(moment.day);

        let time = moment.time.unwrap_or(0);

        let wall = format!(
            "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
            time / 3600,
            time % 3600 / 60,
            time % 60
        );

        let line = match &moment.zone {
            Zone::Utc => format!("{wall} UTC"),

            // a zone this system doesn't know is read as the local clock
            Zone::Named(name) if known_zone(name) => format!("TZ=\"{name}\" {wall}"),

            _ => wall,
        };

        lines.push_str(&line);
        lines.push('\n');
    }

    let converted = date_batch(&lines);

    if converted.len() == moments.len() {
        return converted;
    }

    // `date` failed, so the clock's current offset has to do
    let offset = {
        let clock = Clock::read();

        let now = now();

        clock.day_of(now) * DAY + clock.seconds_into_day(now) - now
    };

    moments
        .iter()
        .map(|moment| {
            let wall = moment.day * DAY + moment.time.unwrap_or(0);

            if moment.zone == Zone::Utc { wall } else { wall - offset }
        })
        .collect()
}

fn known_zone(name: &str) -> bool {
    let plain = name.chars().all(|character| character.is_ascii_alphanumeric() || "/_+-".contains(character));

    plain && !name.contains("..") && Path::new("/usr/share/zoneinfo").join(name).is_file()
}

fn date_batch(lines: &str) -> Vec<i64> {
    let Ok(mut child) = Command::new("date")
        .args(["-f", "-", "+%s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(lines.as_bytes());
    }

    let Ok(output) = child.wait_with_output() else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}
