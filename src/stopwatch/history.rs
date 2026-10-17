use std::fs;
use std::io::Write;

use crate::integrations::state_home;

// sessions shorter than this are not worth keeping
pub const SHORTEST: u64 = 60;

// one finished session: when it started, in seconds since 1970, and how long it ran
#[derive(Clone, Copy)]
pub struct Session {
    pub started: i64,
    pub seconds: u64,
}

// "1760700000 5400" on each line, oldest first
pub fn load() -> Vec<Session> {
    let text = fs::read_to_string(path()).unwrap_or_default();

    let mut sessions = Vec::new();

    for line in text.lines() {
        let mut fields = line.split_whitespace();

        let (Some(started), Some(seconds)) = (fields.next(), fields.next()) else {
            continue;
        };

        let (Ok(started), Ok(seconds)) = (started.parse(), seconds.parse()) else {
            continue;
        };

        sessions.push(Session { started, seconds });
    }

    sessions
}

// appended, so a crash never loses the sessions before it
pub fn add(session: Session) {
    let path = path();

    let _ = fs::create_dir_all(state_home());

    let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) else {
        eprintln!("amane: failed to save the stopwatch session to {path}");

        return;
    };

    let _ = writeln!(file, "{} {}", session.started, session.seconds);
}

/*
 * the session under way, so a restart of the shell keeps counting:
 * "started banked running_since", the last one "-" while paused
 */
pub fn save_current(started: i64, banked: u64, running_since: Option<i64>) {
    let since = running_since.map_or(String::from("-"), |since| since.to_string());

    crate::integrations::write(&current_path(), &format!("{started} {banked} {since}\n"));
}

pub fn clear_current() {
    let _ = fs::remove_file(current_path());
}

pub fn load_current() -> Option<(i64, u64, Option<i64>)> {
    let text = fs::read_to_string(current_path()).ok()?;

    let fields: Vec<&str> = text.split_whitespace().collect();

    let [started, banked, since] = fields.as_slice() else {
        return None;
    };

    Some((started.parse().ok()?, banked.parse().ok()?, since.parse().ok()))
}

fn path() -> String {
    format!("{}/stopwatch-history", state_home())
}

fn current_path() -> String {
    format!("{}/stopwatch-current", state_home())
}

// like "2h 05m", or "45m" under an hour
pub fn duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;

    if hours == 0 {
        return format!("{minutes}m");
    }

    format!("{hours}h {minutes:02}m")
}
