use std::collections::HashSet;

use crate::clock::{civil_date, days_since_1970};

// a recurring event is never walked further than this many times, so a broken rule can't hang
const MOST_STEPS: usize = 20_000;

const DAY: i64 = 24 * 60 * 60;

// a moment as written in the file: a wall clock in some zone, or a whole day
#[derive(Clone, PartialEq)]
pub struct Moment {
    // days since 1970 on the calendar it is written in
    pub day: i64,

    // seconds since midnight, none for an all-day date
    pub time: Option<i64>,

    pub zone: Zone,
}

#[derive(Clone, PartialEq)]
pub enum Zone {
    Utc,

    // an olson name like "Europe/Moscow"
    Named(String),

    // no zone given, the reader's own clock
    Local,
}

// one occurrence before its zone is turned into a real moment
pub struct Occurrence {
    pub title: String,
    pub start: Moment,

    // in seconds, an all-day event lasting whole days
    pub length: i64,
}

// a VEVENT, only the fields that matter here
#[derive(Default)]
struct Raw {
    uid: String,
    title: String,
    start: Option<Moment>,
    end: Option<Moment>,
    duration: Option<i64>,
    rule: Option<String>,
    exceptions: Vec<Moment>,
    recurrence_id: Option<Moment>,
    cancelled: bool,
}

/*
 * every occurrence that starts between the two days, counted in days
 * since 1970; recurring events are walked from their first start, and
 * moved or cancelled occurrences replace the ones they stand for
 */
pub fn occurrences(text: &str, from_day: i64, to_day: i64) -> Vec<Occurrence> {
    let events = parse(text);

    // occurrences that were moved or cancelled, by uid and original start
    let mut replaced: HashSet<(String, i64, Option<i64>)> = HashSet::new();

    for event in &events {
        if let Some(id) = &event.recurrence_id {
            replaced.insert((event.uid.clone(), id.day, id.time));
        }
    }

    let mut found = Vec::new();

    for event in events {
        let Some(start) = event.start.clone() else {
            continue;
        };

        let length = length(&event, &start);

        let title = if event.title.is_empty() {
            String::from("Busy")
        } else {
            event.title.clone()
        };

        // a moved occurrence is its own event, a cancelled one is just gone
        if event.recurrence_id.is_some() || event.rule.is_none() {
            if event.cancelled {
                continue;
            }

            // an event that began earlier still counts while it lasts into the range
            let last_day = start.day + (length - 1).max(0) / DAY;

            if last_day >= from_day && start.day <= to_day {
                found.push(Occurrence { title, start, length });
            }

            continue;
        }

        if event.cancelled {
            continue;
        }

        let rule = event.rule.as_deref().unwrap_or_default();

        for day in expand(rule, &start, to_day) {
            if day < from_day {
                continue;
            }

            let skipped = event
                .exceptions
                .iter()
                .any(|exception| exception.day == day && (exception.time.is_none() || exception.time == start.time));

            if skipped || replaced.contains(&(event.uid.clone(), day, start.time)) {
                continue;
            }

            let moment = Moment { day, ..start.clone() };

            found.push(Occurrence {
                title: title.clone(),
                start: moment,
                length,
            });
        }
    }

    found
}

// from DTEND or DURATION, an all-day event lasting a day and anything else an instant when neither is given
fn length(event: &Raw, start: &Moment) -> i64 {
    if let Some(duration) = event.duration {
        return duration;
    }

    if let Some(end) = &event.end {
        let seconds = (end.day - start.day) * DAY + end.time.unwrap_or(0) - start.time.unwrap_or(0);

        return seconds.max(0);
    }

    if start.time.is_none() { DAY } else { 0 }
}

fn parse(text: &str) -> Vec<Raw> {
    let mut events = Vec::new();

    let mut current: Option<Raw> = None;

    for line in unfold(text) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };

        // "DTSTART;TZID=Europe/Moscow" is the name and its parameters
        let mut parts = name.split(';');

        let key = parts.next().unwrap_or_default().to_ascii_uppercase();

        let parameters: Vec<&str> = parts.collect();

        if key == "BEGIN" && value.eq_ignore_ascii_case("VEVENT") {
            current = Some(Raw::default());

            continue;
        }

        if key == "END" && value.eq_ignore_ascii_case("VEVENT") {
            if let Some(event) = current.take() {
                events.push(event);
            }

            continue;
        }

        let Some(event) = current.as_mut() else {
            continue;
        };

        match key.as_str() {
            "UID" => event.uid = String::from(value),
            "SUMMARY" => event.title = unescape(value),
            "DTSTART" => event.start = moment(value, &parameters),
            "DTEND" => event.end = moment(value, &parameters),
            "DURATION" => event.duration = duration(value),
            "RRULE" => event.rule = Some(value.to_ascii_uppercase()),
            "RECURRENCE-ID" => event.recurrence_id = moment(value, &parameters),
            "STATUS" => event.cancelled = value.eq_ignore_ascii_case("CANCELLED"),

            "EXDATE" => {
                for date in value.split(',') {
                    if let Some(moment) = moment(date, &parameters) {
                        event.exceptions.push(moment);
                    }
                }
            }

            _ => {}
        }
    }

    events
}

// a line that starts with a space or tab goes on the one before it
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();

    for line in text.lines() {
        let line = line.trim_end_matches('\r');

        if let Some(rest) = line.strip_prefix([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);

                continue;
            }
        }

        lines.push(String::from(line));
    }

    lines
}

fn unescape(value: &str) -> String {
    let mut text = String::new();

    let mut characters = value.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);

            continue;
        }

        match characters.next() {
            Some('n' | 'N') => text.push(' '),
            Some(other) => text.push(other),
            None => {}
        }
    }

    text
}

// "20261017", "20261017T150000" or "20261017T150000Z", with an optional TZID parameter
fn moment(value: &str, parameters: &[&str]) -> Option<Moment> {
    let value = value.trim();

    let date = value.get(..8)?;

    let year: i64 = date.get(..4)?.parse().ok()?;
    let month: i64 = date.get(4..6)?.parse().ok()?;
    let day: i64 = date.get(6..8)?.parse().ok()?;

    let day = days_since_1970(year, month, day);

    let Some(time) = value.get(9..15) else {
        return Some(Moment {
            day,
            time: None,
            zone: Zone::Local,
        });
    };

    let hours: i64 = time.get(..2)?.parse().ok()?;
    let minutes: i64 = time.get(2..4)?.parse().ok()?;
    let seconds: i64 = time.get(4..6)?.parse().ok()?;

    let time = Some(hours * 3600 + minutes * 60 + seconds);

    let named = parameters.iter().find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;

        name.eq_ignore_ascii_case("TZID").then(|| value.trim_matches('"'))
    });

    let zone = if value.ends_with('Z') {
        Zone::Utc
    } else if let Some(name) = named {
        Zone::Named(String::from(name))
    } else {
        Zone::Local
    };

    Some(Moment { day, time, zone })
}

// "PT1H30M", "P1D" or "P2W" in seconds
fn duration(value: &str) -> Option<i64> {
    let value = value.trim().trim_start_matches(['+', '-']);

    let rest = value.strip_prefix('P')?;

    let mut total = 0;
    let mut number = String::new();

    for character in rest.chars() {
        if character.is_ascii_digit() {
            number.push(character);

            continue;
        }

        let amount: i64 = if number.is_empty() { 0 } else { number.parse().ok()? };

        number.clear();

        total += match character {
            'W' => amount * 7 * DAY,
            'D' => amount * DAY,
            'H' => amount * 3600,
            'M' => amount * 60,
            'S' => amount,
            _ => 0,
        };
    }

    Some(total)
}

// the parts of an RRULE this understands
struct Rule {
    frequency: String,
    interval: i64,
    count: Option<usize>,

    // the last day an occurrence may start on
    until: Option<i64>,

    // monday 0 to sunday 6, each with an optional nth like the 2 in "2MO"
    weekdays: Vec<(Option<i64>, i64)>,

    month_days: Vec<i64>,
}

fn rule(text: &str) -> Rule {
    let mut rule = Rule {
        frequency: String::new(),
        interval: 1,
        count: None,
        until: None,
        weekdays: Vec::new(),
        month_days: Vec::new(),
    };

    for part in text.split(';') {
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };

        match name {
            "FREQ" => rule.frequency = String::from(value),
            "INTERVAL" => rule.interval = value.parse().unwrap_or(1).max(1),
            "COUNT" => rule.count = value.parse().ok(),
            "UNTIL" => rule.until = moment(value, &[]).map(|moment| moment.day),

            "BYDAY" => {
                for day in value.split(',') {
                    let split = day.len().saturating_sub(2);

                    let (nth, name) = day.split_at(split);

                    let Some(weekday) = weekday(name) else {
                        continue;
                    };

                    rule.weekdays.push((nth.parse().ok(), weekday));
                }
            }

            "BYMONTHDAY" => {
                rule.month_days = value.split(',').filter_map(|day| day.parse().ok()).collect();
            }

            _ => {}
        }
    }

    rule
}

fn weekday(name: &str) -> Option<i64> {
    ["MO", "TU", "WE", "TH", "FR", "SA", "SU"]
        .iter()
        .position(|known| *known == name)
        .map(|index| index as i64)
}

// monday 0 to sunday 6; 1970-01-01 was a thursday
fn weekday_of(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };

    days_since_1970(next_year, next_month, 1) - days_since_1970(year, month, 1)
}

/*
 * the days the rule puts an occurrence on, from the first start up to
 * the last day asked about; the time of day stays the start's own
 */
fn expand(text: &str, start: &Moment, to_day: i64) -> Vec<i64> {
    let rule = rule(text);

    let mut days = Vec::new();

    let last = rule.until.map_or(to_day, |until| until.min(to_day));

    let mut taken = 0;

    // true once the walk is done
    let mut push = |day: i64, days: &mut Vec<i64>| -> bool {
        if day < start.day {
            return false;
        }

        if day > last || rule.count.is_some_and(|count| taken >= count) {
            return true;
        }

        taken += 1;

        days.push(day);

        false
    };

    let (start_year, start_month, start_date) = civil_date(start.day);

    for step in 0..MOST_STEPS as i64 {
        let jump = step * rule.interval;

        let done = match rule.frequency.as_str() {
            "DAILY" => push(start.day + jump, &mut days),

            "WEEKLY" => {
                let monday = start.day - weekday_of(start.day) + jump * 7;

                let mut weekdays: Vec<i64> = rule.weekdays.iter().map(|(_, day)| *day).collect();

                if weekdays.is_empty() {
                    weekdays.push(weekday_of(start.day));
                }

                weekdays.sort_unstable();

                let mut done = false;

                for weekday in weekdays {
                    if push(monday + weekday, &mut days) {
                        done = true;

                        break;
                    }
                }

                done
            }

            "MONTHLY" => {
                let index = start_year * 12 + start_month - 1 + jump;

                let (year, month) = (index.div_euclid(12), index.rem_euclid(12) + 1);

                let mut candidates = month_days(&rule, year, month, start_date);

                candidates.sort_unstable();

                let mut done = false;

                for day in candidates {
                    if push(day, &mut days) {
                        done = true;

                        break;
                    }
                }

                done
            }

            "YEARLY" => {
                let year = start_year + jump;

                // the 29th of february only comes in leap years
                if start_date <= days_in_month(year, start_month) {
                    push(days_since_1970(year, start_month, start_date), &mut days)
                } else {
                    days_since_1970(year, start_month, 1) > last
                }
            }

            _ => true,
        };

        if done {
            break;
        }
    }

    days
}

// the days in one month a monthly rule picks
fn month_days(rule: &Rule, year: i64, month: i64, start_date: i64) -> Vec<i64> {
    let first = days_since_1970(year, month, 1);
    let length = days_in_month(year, month);

    let mut days = Vec::new();

    for (nth, weekday) in &rule.weekdays {
        // every such weekday in the month, then the nth from the start or the end
        let offset = (weekday - weekday_of(first)).rem_euclid(7);

        let all: Vec<i64> = (0..)
            .map(|week| first + offset + week * 7)
            .take_while(|day| *day < first + length)
            .collect();

        match nth {
            Some(nth) if *nth > 0 => days.extend(all.get(*nth as usize - 1)),
            Some(nth) if *nth < 0 => days.extend(all.len().checked_sub(nth.unsigned_abs() as usize).and_then(|index| all.get(index))),
            _ => days.extend(all),
        }
    }

    for date in &rule.month_days {
        let date = if *date < 0 { length + date + 1 } else { *date };

        if (1..=length).contains(&date) {
            days.push(first + date - 1);
        }
    }

    if rule.weekdays.is_empty() && rule.month_days.is_empty() && start_date <= length {
        days.push(first + start_date - 1);
    }

    days
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\n\
BEGIN:VEVENT\r\n\
UID:weekly\r\n\
SUMMARY:Stand\r\n up\r\n\
DTSTART;TZID=Europe/Moscow:20261005T100000\r\n\
DTEND;TZID=Europe/Moscow:20261005T103000\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\n\
EXDATE;TZID=Europe/Moscow:20261012T100000\r\n\
END:VEVENT\r\n\
BEGIN:VEVENT\r\n\
UID:weekly\r\n\
RECURRENCE-ID;TZID=Europe/Moscow:20261014T100000\r\n\
SUMMARY:Moved\r\n\
DTSTART;TZID=Europe/Moscow:20261014T120000\r\n\
DTEND;TZID=Europe/Moscow:20261014T123000\r\n\
END:VEVENT\r\n\
BEGIN:VEVENT\r\n\
UID:holiday\r\n\
SUMMARY:Day off\r\n\
DTSTART;VALUE=DATE:20261016\r\n\
DTEND;VALUE=DATE:20261017\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn expands_weekly_with_exceptions() {
        let from = days_since_1970(2026, 10, 12);
        let to = days_since_1970(2026, 10, 18);

        let found = occurrences(SAMPLE, from, to);

        let mut shown: Vec<(String, i64, Option<i64>)> = found
            .iter()
            .map(|occurrence| (occurrence.title.clone(), occurrence.start.day - from, occurrence.start.time))
            .collect();

        shown.sort();

        // the 12th is excluded, the 14th moved to noon, and the day off is a whole day
        assert_eq!(
            shown,
            vec![
                (String::from("Day off"), 4, None),
                (String::from("Moved"), 2, Some(12 * 3600)),
            ]
        );

        assert_eq!(found.iter().find(|occurrence| occurrence.title == "Day off").map(|o| o.length), Some(DAY));
    }

    #[test]
    fn monthly_nth_weekday() {
        let start = Moment {
            day: days_since_1970(2026, 1, 1),
            time: None,
            zone: Zone::Local,
        };

        let days = expand("FREQ=MONTHLY;BYDAY=-1FR;COUNT=3", &start, days_since_1970(2027, 1, 1));

        let dates: Vec<(i64, i64, i64)> = days.into_iter().map(civil_date).collect();

        assert_eq!(dates, vec![(2026, 1, 30), (2026, 2, 27), (2026, 3, 27)]);
    }

    #[test]
    fn reads_durations() {
        assert_eq!(duration("PT1H30M"), Some(5400));
        assert_eq!(duration("P1D"), Some(DAY));
    }
}
